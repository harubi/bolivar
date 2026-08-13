package sa.ingenious.pdf;

interface CursorBackend<T> extends AutoCloseable {
  T next();

  void cancel();

  @Override
  void close();
}
