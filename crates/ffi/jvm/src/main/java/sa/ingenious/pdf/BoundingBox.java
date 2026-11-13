package sa.ingenious.pdf;

/** A box in PDF space: x grows right, y grows up, {@code x0 <= x1}, {@code y0 <= y1}. */
public record BoundingBox(double x0, double y0, double x1, double y1) {
  public double width() {
    return x1 - x0;
  }

  public double height() {
    return y1 - y0;
  }

  public double area() {
    return width() * height();
  }

  public boolean contains(double x, double y) {
    return x >= x0 && x <= x1 && y >= y0 && y <= y1;
  }

  public boolean intersects(BoundingBox other) {
    return Math.max(x0, other.x0) <= Math.min(x1, other.x1)
        && Math.max(y0, other.y0) <= Math.min(y1, other.y1);
  }
}
