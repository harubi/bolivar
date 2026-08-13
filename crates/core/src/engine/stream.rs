//! Ordered page streams on the shared extraction engine.

use std::any::Any;
use std::collections::BTreeMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};

use crate::arena::PageArena;
use crate::cancellation::CancellationToken;
use crate::document::{PDFDocument, PDFPage};
use crate::error::{PdfError, Result};

use super::plan::ExecutionPlan;
use super::runtime::{Engine, shared_engine};

type Precheck<R> =
    dyn Fn(usize, &PDFPage, &PDFDocument, &CancellationToken) -> Result<Option<R>> + Send + Sync;
type PageWork<R> = dyn Fn(&mut PageArena, usize, &PDFPage, &PDFDocument, &CancellationToken) -> Result<R>
    + Send
    + Sync;

enum StreamMessage<R> {
    Completed {
        position: usize,
        result: Result<R>,
        arena: Box<PageArena>,
    },
    Wake,
}

struct Scheduler<R> {
    engine: Arc<Engine>,
    document: Arc<PDFDocument>,
    order: Arc<[usize]>,
    precheck: Arc<Precheck<R>>,
    page_work: Arc<PageWork<R>>,
    sender: Sender<StreamMessage<R>>,
    cancellation: CancellationToken,
}

impl<R: Send + 'static> Scheduler<R> {
    fn schedule(self: &Arc<Self>, position: usize, arena: Box<PageArena>) {
        let scheduler = Arc::clone(self);
        self.engine
            .spawn(move || scheduler.run_page(position, arena));
    }

    fn run_page(&self, position: usize, mut arena: Box<PageArena>) {
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
                    (self.page_work)(
                        &mut arena,
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
            let _ = self.sender.send(StreamMessage::Completed {
                position,
                result,
                arena,
            });
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
    pub fn cancel(&self) {
        self.token.cancel();
        (self.wake)();
    }

    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.token.is_cancelled()
    }
}

struct CompletedPage<R> {
    result: Result<R>,
    arena: Box<PageArena>,
}

/// An ordered stream with a fixed number of page slots.
pub struct Stream<R: Send + 'static> {
    receiver: Receiver<StreamMessage<R>>,
    scheduler: Arc<Scheduler<R>>,
    cancellation: CancellationHandle,
    order: Arc<[usize]>,
    next_position: usize,
    next_schedule_position: usize,
    completed: BTreeMap<usize, CompletedPage<R>>,
    pending_arena: Option<Box<PageArena>>,
    failed: bool,
}

impl<R: Send + 'static> Stream<R> {
    fn new(scheduler: Arc<Scheduler<R>>, receiver: Receiver<StreamMessage<R>>) -> Self {
        let sender = scheduler.sender.clone();
        let cancellation = CancellationHandle {
            token: scheduler.cancellation.clone(),
            wake: Arc::new(move || {
                let _ = sender.send(StreamMessage::Wake);
            }),
        };
        let order = Arc::clone(&scheduler.order);
        let initial_slots = scheduler.engine.worker_count().min(order.len());
        for position in 0..initial_slots {
            scheduler.schedule(position, Box::default());
        }

        Self {
            receiver,
            scheduler,
            cancellation,
            order,
            next_position: 0,
            next_schedule_position: initial_slots,
            completed: BTreeMap::new(),
            pending_arena: None,
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

    fn replenish_slot(&mut self) {
        let Some(arena) = self.pending_arena.take() else {
            return;
        };
        if !self.cancellation.is_cancelled() && self.next_schedule_position < self.order.len() {
            let position = self.next_schedule_position;
            self.next_schedule_position += 1;
            self.scheduler.schedule(position, arena);
        }
    }

    fn fail(&mut self, error: PdfError) -> Option<Result<(usize, R)>> {
        self.failed = true;
        self.cancellation.cancel();
        self.pending_arena.take();
        self.completed.clear();
        Some(Err(error))
    }
}

impl<R: Send + 'static> Iterator for Stream<R> {
    type Item = Result<(usize, R)>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.failed {
            return None;
        }
        self.replenish_slot();
        if self.next_position >= self.order.len() {
            return None;
        }

        loop {
            if self.cancellation.is_cancelled() {
                return self.fail(PdfError::Cancelled);
            }

            if let Some(completed) = self.completed.remove(&self.next_position) {
                let page_index = self.order[self.next_position];
                self.next_position += 1;
                self.pending_arena = Some(completed.arena);
                return match completed.result {
                    Ok(result) => Some(Ok((page_index, result))),
                    Err(error) => self.fail(error),
                };
            }

            match self.receiver.recv() {
                Ok(StreamMessage::Completed {
                    position,
                    result,
                    arena,
                }) => {
                    if position < self.next_position || position >= self.order.len() {
                        return self.fail(PdfError::RuntimeError(format!(
                            "stream received invalid result position {position}"
                        )));
                    }
                    if self
                        .completed
                        .insert(position, CompletedPage { result, arena })
                        .is_some()
                    {
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
        precheck,
        page_work,
    )
}

pub(crate) fn run_stream_on_engine<R, P, F>(
    engine: Arc<Engine>,
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
    let plan = ExecutionPlan::new(
        document.page_index().len(),
        page_numbers.as_deref(),
        maxpages,
    );
    let order: Arc<[usize]> = plan.order.into();
    // Page slots bound the queue. An unbounded channel keeps a completed
    // worker from blocking the shared pool when its cursor is not read.
    let (sender, receiver) = channel();
    let scheduler = Arc::new(Scheduler {
        engine,
        document,
        order,
        precheck: Arc::new(precheck),
        page_work: Arc::new(page_work),
        sender,
        cancellation: CancellationToken::new(),
    });
    Ok(Stream::new(scheduler, receiver))
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
    fn local_engine_preserves_order_and_uses_one_slot_per_worker() {
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
        assert_eq!(started.load(Ordering::SeqCst), 2);
        assert_eq!(stream.next_schedule_position, 2);
        assert!(stream.completed.len() <= 1);

        let mut pages = vec![0];
        pages.extend(stream.map(|result| result.expect("page result").0));
        assert_eq!(pages, vec![0, 1, 2, 3, 4]);
        assert!(max_active.load(Ordering::SeqCst) <= 2);
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
    fn cancellation_wakes_an_active_boundary_call() {
        let engine = Engine::new(1).expect("engine");
        let (started_sender, started_receiver) = std::sync::mpsc::channel();
        let mut stream = run_stream_on_engine(
            engine,
            test_document(),
            None,
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
    fn worker_panic_is_a_runtime_error_and_does_not_break_the_engine() {
        let engine = Engine::new(1).expect("engine");
        let mut failed_stream = run_stream_on_engine(
            Arc::clone(&engine),
            test_document(),
            None,
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
