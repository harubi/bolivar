//! Ordered page streams on the shared extraction engine.

use std::any::Any;
use std::collections::BTreeMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::mpsc::channel;
use std::sync::{Arc, Once};

use hotpath::wrap::std::sync::Mutex;
use hotpath::wrap::std::sync::mpsc::{Receiver, Sender};

use crate::arena::PageArena;
use crate::cancellation::CancellationToken;
use crate::document::{PDFDocument, PDFPage};
use crate::error::{PdfError, Result};

use super::plan::ExecutionPlan;
use super::runtime::{Engine, shared_engine};

pub const DEFAULT_STREAM_WINDOW_CAPACITY: usize = 50;

type Precheck<R> =
    dyn Fn(usize, &PDFPage, &PDFDocument, &CancellationToken) -> Result<Option<R>> + Send + Sync;
type PageWork<R> = dyn Fn(&mut PageArena, usize, &PDFPage, &PDFDocument, &CancellationToken) -> Result<R>
    + Send
    + Sync;

trait PageWorker<R>: Send {
    fn run(
        &mut self,
        arena: &mut PageArena,
        page_index: usize,
        page: &PDFPage,
        document: &PDFDocument,
        cancellation: &CancellationToken,
    ) -> Result<R>;
}

trait WorkerFactory<R>: Send + Sync {
    fn create(&self) -> Box<dyn PageWorker<R>>;
}

struct SharedWorker<R> {
    page_work: Arc<PageWork<R>>,
}

impl<R> PageWorker<R> for SharedWorker<R> {
    fn run(
        &mut self,
        arena: &mut PageArena,
        page_index: usize,
        page: &PDFPage,
        document: &PDFDocument,
        cancellation: &CancellationToken,
    ) -> Result<R> {
        (self.page_work)(arena, page_index, page, document, cancellation)
    }
}

struct SharedFactory<R> {
    page_work: Arc<PageWork<R>>,
}

impl<R: 'static> WorkerFactory<R> for SharedFactory<R> {
    fn create(&self) -> Box<dyn PageWorker<R>> {
        Box::new(SharedWorker {
            page_work: Arc::clone(&self.page_work),
        })
    }
}

type StateInit<S> = dyn Fn() -> S + Send + Sync;
type StatefulWork<R, S> = dyn Fn(&mut S, &mut PageArena, usize, &PDFPage, &PDFDocument, &CancellationToken) -> Result<R>
    + Send
    + Sync;

struct StreamRuntime {
    engine: Arc<Engine>,
    window_capacity: usize,
}

struct StatefulWorker<R, S> {
    state: S,
    page_work: Arc<StatefulWork<R, S>>,
}

impl<R, S: Send> PageWorker<R> for StatefulWorker<R, S> {
    fn run(
        &mut self,
        arena: &mut PageArena,
        page_index: usize,
        page: &PDFPage,
        document: &PDFDocument,
        cancellation: &CancellationToken,
    ) -> Result<R> {
        (self.page_work)(
            &mut self.state,
            arena,
            page_index,
            page,
            document,
            cancellation,
        )
    }
}

struct StatefulFactory<R, S> {
    init_state: Arc<StateInit<S>>,
    page_work: Arc<StatefulWork<R, S>>,
}

impl<R: 'static, S: Send + 'static> WorkerFactory<R> for StatefulFactory<R, S> {
    fn create(&self) -> Box<dyn PageWorker<R>> {
        Box::new(StatefulWorker {
            state: (self.init_state)(),
            page_work: Arc::clone(&self.page_work),
        })
    }
}

enum StreamMessage<R> {
    Completed { position: usize, result: Result<R> },
    Wake,
}

struct SchedulerState {
    next_work_position: usize,
    schedule_limit: usize,
    active_workers: usize,
}

struct Scheduler<R> {
    engine: Arc<Engine>,
    document: Arc<PDFDocument>,
    order: Arc<[usize]>,
    precheck: Arc<Precheck<R>>,
    worker_factory: Arc<dyn WorkerFactory<R>>,
    sender: Sender<StreamMessage<R>>,
    cancellation: CancellationToken,
    window_capacity: usize,
    state: Mutex<SchedulerState>,
    workers: Mutex<Vec<Box<dyn PageWorker<R>>>>,
}

impl<R: Send + 'static> Scheduler<R> {
    fn start_workers(self: &Arc<Self>) {
        let worker_count = {
            let mut state = self.state.lock().expect("stream scheduler state");
            let available_work = state
                .schedule_limit
                .saturating_sub(state.next_work_position);
            let worker_count = self
                .engine
                .worker_count()
                .saturating_sub(state.active_workers)
                .min(available_work);
            state.active_workers += worker_count;
            worker_count
        };

        for _ in 0..worker_count {
            let scheduler = Arc::clone(self);
            self.engine.spawn(move || scheduler.run_worker());
        }
    }

    fn advance_window(self: &Arc<Self>, next_position: usize) {
        {
            let mut state = self.state.lock().expect("stream scheduler state");
            state.schedule_limit = next_position
                .saturating_add(self.window_capacity)
                .min(self.order.len());
        }
        self.start_workers();
    }

    fn claim_position(&self) -> Option<usize> {
        if self.cancellation.is_cancelled() {
            return None;
        }
        let mut state = self.state.lock().expect("stream scheduler state");
        if state.next_work_position >= state.schedule_limit {
            return None;
        }
        let position = state.next_work_position;
        state.next_work_position += 1;
        Some(position)
    }

    fn worker_finished(&self) {
        let mut state = self.state.lock().expect("stream scheduler state");
        state.active_workers = state
            .active_workers
            .checked_sub(1)
            .expect("active stream worker");
    }

    #[hotpath::measure]
    fn run_worker(self: Arc<Self>) {
        let mut arena = PageArena::new();
        let mut worker = self
            .workers
            .lock()
            .expect("stream worker state")
            .pop()
            .unwrap_or_else(|| self.worker_factory.create());
        while let Some(position) = self.claim_position() {
            self.run_page(position, &mut arena, worker.as_mut());
        }
        self.workers
            .lock()
            .expect("stream worker state")
            .push(worker);
        self.worker_finished();
        if !self.cancellation.is_cancelled() {
            self.start_workers();
        }
    }

    #[hotpath::measure]
    fn run_page(&self, position: usize, arena: &mut PageArena, worker: &mut dyn PageWorker<R>) {
        let work = catch_unwind(AssertUnwindSafe(|| -> Result<R> {
            self.cancellation.check()?;
            let page_index = self.order[position];
            let page = self.document.get_page_cached(page_index)?;
            self.cancellation.check()?;

            match (self.precheck)(
                page_index,
                page.as_ref(),
                self.document.as_ref(),
                &self.cancellation,
            )? {
                Some(result) => Ok(result),
                None => {
                    self.cancellation.check()?;
                    arena.reset();
                    worker.run(
                        arena,
                        page_index,
                        page.as_ref(),
                        self.document.as_ref(),
                        &self.cancellation,
                    )
                }
            }
        }));

        let result = match work {
            Ok(result) => result,
            Err(payload) => Err(PdfError::RuntimeError(format!(
                "page worker panicked: {}",
                panic_message(payload.as_ref())
            ))),
        };

        if !self.cancellation.is_cancelled() {
            let _ = self
                .sender
                .send(StreamMessage::Completed { position, result });
        }
    }
}

fn panic_message(payload: &(dyn Any + Send)) -> &str {
    if let Some(message) = payload.downcast_ref::<&str>() {
        message
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message.as_str()
    } else {
        "unknown panic payload"
    }
}

/// A cloneable handle that stops a stream and wakes an active boundary call.
#[derive(Clone)]
pub struct CancellationHandle {
    token: CancellationToken,
    wake: Arc<dyn Fn() + Send + Sync>,
}

impl CancellationHandle {
    fn new<R: Send + 'static>(token: CancellationToken, sender: Sender<StreamMessage<R>>) -> Self {
        let wake_once = Once::new();
        Self {
            token,
            wake: Arc::new(move || {
                wake_once.call_once(|| {
                    let _ = sender.send(StreamMessage::Wake);
                });
            }),
        }
    }

    pub fn cancel(&self) {
        self.token.cancel();
        (self.wake)();
    }

    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.token.is_cancelled()
    }
}

/// An ordered stream with a fixed number of page slots.
pub struct Stream<R: Send + 'static> {
    receiver: Receiver<StreamMessage<R>>,
    scheduler: Arc<Scheduler<R>>,
    cancellation: CancellationHandle,
    next_position: usize,
    completed: BTreeMap<usize, Result<R>>,
    failed: bool,
}

impl<R: Send + 'static> Stream<R> {
    fn new(scheduler: Arc<Scheduler<R>>, receiver: Receiver<StreamMessage<R>>) -> Self {
        let cancellation =
            CancellationHandle::new(scheduler.cancellation.clone(), scheduler.sender.clone());
        scheduler.start_workers();

        Self {
            receiver,
            scheduler,
            cancellation,
            next_position: 0,
            completed: BTreeMap::new(),
            failed: false,
        }
    }

    #[must_use]
    pub fn cancellation_handle(&self) -> CancellationHandle {
        self.cancellation.clone()
    }

    pub fn cancel(&self) {
        self.cancellation.cancel();
    }

    fn fail(&mut self, error: PdfError) -> Option<Result<(usize, R)>> {
        self.failed = true;
        self.cancellation.cancel();
        self.completed.clear();
        Some(Err(error))
    }
}

impl<R: Send + 'static> Iterator for Stream<R> {
    type Item = Result<(usize, R)>;

    #[hotpath::measure]
    fn next(&mut self) -> Option<Self::Item> {
        if self.failed {
            return None;
        }
        if self.next_position >= self.scheduler.order.len() {
            return None;
        }

        loop {
            if self.cancellation.is_cancelled() {
                return self.fail(PdfError::Cancelled);
            }

            if let Some(result) = self.completed.remove(&self.next_position) {
                let page_index = self.scheduler.order[self.next_position];
                self.next_position += 1;
                return match result {
                    Ok(result) => {
                        self.scheduler.advance_window(self.next_position);
                        Some(Ok((page_index, result)))
                    }
                    Err(error) => self.fail(error),
                };
            }

            match self.receiver.recv() {
                Ok(StreamMessage::Completed { position, result }) => {
                    if position < self.next_position || position >= self.scheduler.order.len() {
                        return self.fail(PdfError::RuntimeError(format!(
                            "stream received invalid result position {position}"
                        )));
                    }
                    if self.completed.insert(position, result).is_some() {
                        return self.fail(PdfError::RuntimeError(format!(
                            "stream received duplicate result position {position}"
                        )));
                    }
                }
                Ok(StreamMessage::Wake) => {}
                Err(_) => {
                    return self.fail(PdfError::RuntimeError(
                        "stream closed before all page results arrived".to_string(),
                    ));
                }
            }
        }
    }
}

impl<R: Send + 'static> Drop for Stream<R> {
    fn drop(&mut self) {
        self.cancellation.cancel();
    }
}

/// Run a stream whose callbacks can check the stream cancellation token.
pub fn run_stream_cancellable<R, P, F>(
    document: Arc<PDFDocument>,
    page_numbers: Option<Vec<usize>>,
    maxpages: usize,
    precheck: P,
    page_work: F,
) -> Result<Stream<R>>
where
    R: Send + 'static,
    P: Fn(usize, &PDFPage, &PDFDocument, &CancellationToken) -> Result<Option<R>>
        + Send
        + Sync
        + 'static,
    F: Fn(&mut PageArena, usize, &PDFPage, &PDFDocument, &CancellationToken) -> Result<R>
        + Send
        + Sync
        + 'static,
{
    run_stream_on_engine(
        shared_engine()?,
        document,
        page_numbers,
        maxpages,
        DEFAULT_STREAM_WINDOW_CAPACITY,
        precheck,
        page_work,
    )
}

pub(crate) fn run_stream_on_engine<R, P, F>(
    engine: Arc<Engine>,
    document: Arc<PDFDocument>,
    page_numbers: Option<Vec<usize>>,
    maxpages: usize,
    window_capacity: usize,
    precheck: P,
    page_work: F,
) -> Result<Stream<R>>
where
    R: Send + 'static,
    P: Fn(usize, &PDFPage, &PDFDocument, &CancellationToken) -> Result<Option<R>>
        + Send
        + Sync
        + 'static,
    F: Fn(&mut PageArena, usize, &PDFPage, &PDFDocument, &CancellationToken) -> Result<R>
        + Send
        + Sync
        + 'static,
{
    let page_work: Arc<PageWork<R>> = Arc::new(page_work);
    let worker_factory = Arc::new(SharedFactory { page_work });
    run_stream_with_factory(
        engine,
        document,
        page_numbers,
        maxpages,
        window_capacity,
        precheck,
        worker_factory,
    )
}

fn run_stream_with_factory<R, P>(
    engine: Arc<Engine>,
    document: Arc<PDFDocument>,
    page_numbers: Option<Vec<usize>>,
    maxpages: usize,
    window_capacity: usize,
    precheck: P,
    worker_factory: Arc<dyn WorkerFactory<R>>,
) -> Result<Stream<R>>
where
    R: Send + 'static,
    P: Fn(usize, &PDFPage, &PDFDocument, &CancellationToken) -> Result<Option<R>>
        + Send
        + Sync
        + 'static,
{
    let plan = ExecutionPlan::new(
        document.page_index().len(),
        page_numbers.as_deref(),
        maxpages,
    );
    let order: Arc<[usize]> = plan.order.into();
    let window_capacity = window_capacity.max(1);
    let schedule_limit = window_capacity.min(order.len());
    // The sliding window bounds the queue. An unbounded channel keeps a
    // completed worker from blocking the shared pool when its cursor is idle.
    let (sender, receiver) =
        hotpath::channel!(channel::<StreamMessage<R>>(), label = "page-results");
    let scheduler = Arc::new(Scheduler {
        engine,
        document,
        order,
        precheck: Arc::new(precheck),
        worker_factory,
        sender,
        cancellation: CancellationToken::new(),
        window_capacity,
        state: hotpath::mutex!(
            std::sync::Mutex::new(SchedulerState {
                next_work_position: 0,
                schedule_limit,
                active_workers: 0,
            }),
            label = "stream-scheduler"
        ),
        workers: hotpath::mutex!(std::sync::Mutex::new(Vec::new()), label = "stream-workers"),
    });
    Ok(Stream::new(scheduler, receiver))
}

pub(crate) fn run_stateful_stream<R, P, S, I, F>(
    document: Arc<PDFDocument>,
    page_numbers: Option<Vec<usize>>,
    maxpages: usize,
    precheck: P,
    init_state: I,
    page_work: F,
) -> Result<Stream<R>>
where
    R: Send + 'static,
    P: Fn(usize, &PDFPage, &PDFDocument, &CancellationToken) -> Result<Option<R>>
        + Send
        + Sync
        + 'static,
    S: Send + 'static,
    I: Fn() -> S + Send + Sync + 'static,
    F: Fn(&mut S, &mut PageArena, usize, &PDFPage, &PDFDocument, &CancellationToken) -> Result<R>
        + Send
        + Sync
        + 'static,
{
    run_stream_stateful_on_engine(
        StreamRuntime {
            engine: shared_engine()?,
            window_capacity: DEFAULT_STREAM_WINDOW_CAPACITY,
        },
        document,
        page_numbers,
        maxpages,
        precheck,
        init_state,
        page_work,
    )
}

fn run_stream_stateful_on_engine<R, P, S, I, F>(
    runtime: StreamRuntime,
    document: Arc<PDFDocument>,
    page_numbers: Option<Vec<usize>>,
    maxpages: usize,
    precheck: P,
    init_state: I,
    page_work: F,
) -> Result<Stream<R>>
where
    R: Send + 'static,
    P: Fn(usize, &PDFPage, &PDFDocument, &CancellationToken) -> Result<Option<R>>
        + Send
        + Sync
        + 'static,
    S: Send + 'static,
    I: Fn() -> S + Send + Sync + 'static,
    F: Fn(&mut S, &mut PageArena, usize, &PDFPage, &PDFDocument, &CancellationToken) -> Result<R>
        + Send
        + Sync
        + 'static,
{
    let worker_factory = Arc::new(StatefulFactory {
        init_state: Arc::new(init_state),
        page_work: Arc::new(page_work),
    });
    run_stream_with_factory(
        runtime.engine,
        document,
        page_numbers,
        maxpages,
        runtime.window_capacity,
        precheck,
        worker_factory,
    )
}

/// Run a stream with callbacks that do not need the cancellation token.
pub fn run_stream<R, P, F>(
    document: Arc<PDFDocument>,
    page_numbers: Option<Vec<usize>>,
    maxpages: usize,
    precheck: P,
    page_work: F,
) -> Result<Stream<R>>
where
    R: Send + 'static,
    P: Fn(usize, &PDFPage, &PDFDocument) -> Result<Option<R>> + Send + Sync + 'static,
    F: Fn(&mut PageArena, usize, &PDFPage, &PDFDocument) -> Result<R> + Send + Sync + 'static,
{
    run_stream_cancellable(
        document,
        page_numbers,
        maxpages,
        move |page_index, page, document, _| precheck(page_index, page, document),
        move |arena, page_index, page, document, _| page_work(arena, page_index, page, document),
    )
}

pub fn no_precheck<R>(
    _page_index: usize,
    _page: &PDFPage,
    _document: &PDFDocument,
) -> Result<Option<R>> {
    Ok(None)
}

pub fn no_precheck_cancellable<R>(
    _page_index: usize,
    _page: &PDFPage,
    _document: &PDFDocument,
    cancellation: &CancellationToken,
) -> Result<Option<R>> {
    cancellation.check()?;
    Ok(None)
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::{Arc, Condvar, Mutex};
    use std::time::Duration;

    use crate::document::PDFDocument;
    use crate::document::catalog::DEFAULT_CACHE_CAPACITY;
    use crate::error::PdfError;

    use super::*;

    fn test_document() -> Arc<PDFDocument> {
        Arc::new(
            PDFDocument::new_from_vec_with_cache(
                include_bytes!("../../tests/fixtures/contrib/pagelabels.pdf").to_vec(),
                "",
                DEFAULT_CACHE_CAPACITY,
            )
            .expect("test document"),
        )
    }

    struct ActiveWork {
        active: Arc<AtomicUsize>,
    }

    impl Drop for ActiveWork {
        fn drop(&mut self) {
            self.active.fetch_sub(1, Ordering::SeqCst);
        }
    }

    #[test]
    fn local_engine_preserves_order_and_limits_active_work() {
        let engine = Engine::new(2).expect("engine");
        let active = Arc::new(AtomicUsize::new(0));
        let max_active = Arc::new(AtomicUsize::new(0));
        let started = Arc::new(AtomicUsize::new(0));
        let second_started = Arc::new((Mutex::new(false), Condvar::new()));

        let mut stream = run_stream_on_engine(
            engine,
            test_document(),
            None,
            0,
            2,
            no_precheck_cancellable::<usize>,
            {
                let active = Arc::clone(&active);
                let max_active = Arc::clone(&max_active);
                let started = Arc::clone(&started);
                let second_started = Arc::clone(&second_started);
                move |_arena, page_index, _page, _document, cancellation| {
                    let active_count = active.fetch_add(1, Ordering::SeqCst) + 1;
                    max_active.fetch_max(active_count, Ordering::SeqCst);
                    let _active_work = ActiveWork {
                        active: Arc::clone(&active),
                    };
                    started.fetch_add(1, Ordering::SeqCst);

                    if page_index == 1 {
                        let (lock, ready) = &*second_started;
                        *lock.lock().expect("second-page lock") = true;
                        ready.notify_all();
                    } else if page_index == 0 {
                        let (lock, ready) = &*second_started;
                        let mut is_ready = lock.lock().expect("first-page lock");
                        while !*is_ready {
                            cancellation.check()?;
                            let (guard, _) = ready
                                .wait_timeout(is_ready, Duration::from_millis(10))
                                .expect("first-page wait");
                            is_ready = guard;
                        }
                    }
                    Ok(page_index)
                }
            },
        )
        .expect("stream");

        assert_eq!(
            stream.next().expect("first result").expect("first page"),
            (0, 0)
        );
        assert!(started.load(Ordering::SeqCst) >= 2);
        assert!(stream.completed.len() <= 2);

        let mut pages = vec![0];
        pages.extend(stream.map(|result| result.expect("page result").0));
        assert_eq!(pages, vec![0, 1, 2, 3, 4]);
        assert!(max_active.load(Ordering::SeqCst) <= 2);
    }

    #[test]
    fn stateful_worker_reuses_state_across_pages() {
        let engine = Engine::new(1).expect("engine");
        let stream = run_stream_stateful_on_engine(
            StreamRuntime {
                engine,
                window_capacity: 1,
            },
            test_document(),
            None,
            0,
            no_precheck_cancellable::<usize>,
            || 0usize,
            |count, _arena, _page_index, _page, _document, _cancellation| {
                *count += 1;
                Ok(*count)
            },
        )
        .expect("stream");

        let values = stream
            .map(|result| result.expect("page result").1)
            .collect::<Vec<_>>();

        assert_eq!(values, vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn unread_stream_does_not_block_another_stream_on_one_worker() {
        let engine = Engine::new(1).expect("engine");
        let (completed_sender, completed_receiver) = std::sync::mpsc::channel();
        let _unread_stream = run_stream_on_engine(
            Arc::clone(&engine),
            test_document(),
            None,
            1,
            1,
            no_precheck_cancellable::<usize>,
            move |_arena, page_index, _page, _document, _cancellation| {
                completed_sender.send(()).expect("completion signal");
                Ok(page_index)
            },
        )
        .expect("unread stream");
        completed_receiver
            .recv_timeout(Duration::from_secs(2))
            .expect("first stream work");

        let mut second_stream = run_stream_on_engine(
            engine,
            test_document(),
            None,
            1,
            1,
            no_precheck_cancellable::<usize>,
            |_arena, page_index, _page, _document, _cancellation| Ok(page_index),
        )
        .expect("second stream");
        assert_eq!(
            second_stream
                .next()
                .expect("second result")
                .expect("second page"),
            (0, 0)
        );
    }

    #[test]
    fn unread_stream_stops_at_window_capacity() {
        let engine = Engine::new(4).expect("engine");
        let (started_sender, started_receiver) = std::sync::mpsc::channel();
        let _stream = run_stream_on_engine(
            engine,
            test_document(),
            None,
            0,
            2,
            no_precheck_cancellable::<usize>,
            move |_arena, page_index, _page, _document, _cancellation| {
                started_sender.send(page_index).expect("start signal");
                Ok(page_index)
            },
        )
        .expect("stream");

        started_receiver
            .recv_timeout(Duration::from_secs(2))
            .expect("first window page");
        started_receiver
            .recv_timeout(Duration::from_secs(2))
            .expect("second window page");
        assert!(
            started_receiver
                .recv_timeout(Duration::from_millis(50))
                .is_err()
        );
    }

    #[test]
    fn cancellation_wakes_an_active_boundary_call() {
        let engine = Engine::new(1).expect("engine");
        let (started_sender, started_receiver) = std::sync::mpsc::channel();
        let mut stream = run_stream_on_engine(
            engine,
            test_document(),
            None,
            1,
            1,
            no_precheck_cancellable::<usize>,
            move |_arena, page_index, _page, _document, cancellation| {
                started_sender.send(()).expect("start signal");
                while !cancellation.is_cancelled() {
                    std::thread::park_timeout(Duration::from_millis(5));
                }
                cancellation.check()?;
                Ok(page_index)
            },
        )
        .expect("stream");
        let cancellation = stream.cancellation_handle();
        let (result_sender, result_receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            result_sender
                .send(stream.next().expect("one stream result"))
                .expect("result signal");
        });

        started_receiver
            .recv_timeout(Duration::from_secs(2))
            .expect("worker started");
        cancellation.cancel();
        let result = result_receiver
            .recv_timeout(Duration::from_secs(2))
            .expect("cancelled result");
        assert!(matches!(result, Err(PdfError::Cancelled)));
    }

    #[test]
    fn repeated_cancellation_sends_one_wake() {
        let (sender, receiver) =
            hotpath::channel!(channel::<StreamMessage<()>>(), label = "cancellation-wake");
        let cancellation = CancellationHandle::new(CancellationToken::new(), sender);

        cancellation.cancel();
        cancellation.cancel();

        assert!(cancellation.is_cancelled());
        assert!(matches!(receiver.try_recv(), Ok(StreamMessage::Wake)));
        assert!(matches!(
            receiver.try_recv(),
            Err(std::sync::mpsc::TryRecvError::Empty)
        ));
    }

    #[test]
    fn worker_panic_is_a_runtime_error_and_does_not_break_the_engine() {
        let engine = Engine::new(1).expect("engine");
        let mut failed_stream = run_stream_on_engine(
            Arc::clone(&engine),
            test_document(),
            None,
            1,
            1,
            no_precheck_cancellable::<usize>,
            |_arena, _page_index, _page, _document, _cancellation| -> Result<usize> {
                panic!("test worker panic")
            },
        )
        .expect("failed stream");
        let error = failed_stream
            .next()
            .expect("failed result")
            .expect_err("runtime error");
        assert!(matches!(
            error,
            PdfError::RuntimeError(message) if message.contains("test worker panic")
        ));

        let mut healthy_stream = run_stream_on_engine(
            engine,
            test_document(),
            None,
            1,
            1,
            no_precheck_cancellable::<usize>,
            |_arena, page_index, _page, _document, _cancellation| Ok(page_index),
        )
        .expect("healthy stream");
        assert!(healthy_stream.next().expect("healthy result").is_ok());
    }

    #[test]
    fn first_error_stops_new_page_work() {
        let engine = Engine::new(2).expect("engine");
        let started = Arc::new(AtomicUsize::new(0));
        let later_page_started = Arc::new(AtomicBool::new(false));
        let mut stream = run_stream_on_engine(
            engine,
            test_document(),
            None,
            0,
            2,
            no_precheck_cancellable::<usize>,
            {
                let started = Arc::clone(&started);
                let later_page_started = Arc::clone(&later_page_started);
                move |_arena, page_index, _page, _document, cancellation| {
                    started.fetch_add(1, Ordering::SeqCst);
                    if page_index >= 2 {
                        later_page_started.store(true, Ordering::SeqCst);
                    }
                    if page_index == 0 {
                        Err(PdfError::InvalidArgument("test failure".to_string()))
                    } else {
                        while !cancellation.is_cancelled() {
                            std::thread::park_timeout(Duration::from_millis(5));
                        }
                        cancellation.check()?;
                        Ok(page_index)
                    }
                }
            },
        )
        .expect("stream");

        assert!(matches!(
            stream.next().expect("failed result"),
            Err(PdfError::InvalidArgument(_))
        ));
        std::thread::sleep(Duration::from_millis(20));
        assert!(started.load(Ordering::SeqCst) <= 2);
        assert!(!later_page_started.load(Ordering::SeqCst));
    }
}
