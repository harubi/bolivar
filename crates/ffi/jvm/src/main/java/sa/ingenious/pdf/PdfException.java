package sa.ingenious.pdf;

import java.io.Serial;

/**
 * A failure reported by Bolivar. Each subclass matches one native status; {@link #code()} is that
 * status and {@link #field()} names the option at fault for {@link InvalidArgument}.
 */
public abstract sealed class PdfException extends RuntimeException
    permits PdfException.InvalidPath,
        PdfException.InvalidArgument,
        PdfException.IoNotFound,
        PdfException.IoPermissionDenied,
        PdfException.IoError,
        PdfException.SyntaxError,
        PdfException.EncryptionError,
        PdfException.MalformedPdf,
        PdfException.DecodeError,
        PdfException.Cancelled,
        PdfException.RuntimeError,
        PdfException.NativeError {
  @Serial private static final long serialVersionUID = 1L;

  private final int code;
  private final String field;

  PdfException(int code, String message, String field, Throwable cause) {
    super(message, cause);
    this.code = code;
    this.field = field;
  }

  /** The native status code: 1 to 12, as {@code crates/ffi/src/abi.rs} defines them. */
  public int code() {
    return code;
  }

  /** The option field at fault, or null. */
  public String field() {
    return field;
  }

  /**
   * The exception for a native status. Public only so the internal native layer can reach it;
   * not meant for callers.
   */
  public static PdfException of(int code, String message, String field) {
    String text = message == null || message.isEmpty() ? "native error " + code : message;
    return switch (code) {
      case 1 -> new InvalidPath(text);
      case 2 -> new InvalidArgument(text, field);
      case 3 -> new IoNotFound(text);
      case 4 -> new IoPermissionDenied(text);
      case 5 -> new IoError(text, null);
      case 6 -> new SyntaxError(text);
      case 7 -> new EncryptionError(text);
      case 8 -> new MalformedPdf(text);
      case 9 -> new DecodeError(text);
      case 10 -> new Cancelled(text);
      case 11 -> new RuntimeError(text);
      default -> new NativeError(code, text, null);
    };
  }

  /** The path is empty, holds a NUL, or is a URL. */
  public static final class InvalidPath extends PdfException {
    @Serial private static final long serialVersionUID = 1L;

    public InvalidPath(String message) {
      super(1, message, null, null);
    }
  }

  /** An option or argument is invalid; {@link #field()} names it. */
  public static final class InvalidArgument extends PdfException {
    @Serial private static final long serialVersionUID = 1L;

    public InvalidArgument(String message, String field) {
      super(2, message, field, null);
    }
  }

  public static final class IoNotFound extends PdfException {
    @Serial private static final long serialVersionUID = 1L;

    public IoNotFound(String message) {
      super(3, message, null, null);
    }
  }

  public static final class IoPermissionDenied extends PdfException {
    @Serial private static final long serialVersionUID = 1L;

    public IoPermissionDenied(String message) {
      super(4, message, null, null);
    }
  }

  public static final class IoError extends PdfException {
    @Serial private static final long serialVersionUID = 1L;

    public IoError(String message, Throwable cause) {
      super(5, message, null, cause);
    }
  }

  public static final class SyntaxError extends PdfException {
    @Serial private static final long serialVersionUID = 1L;

    public SyntaxError(String message) {
      super(6, message, null, null);
    }
  }

  /** The document is encrypted and the password is missing or wrong. */
  public static final class EncryptionError extends PdfException {
    @Serial private static final long serialVersionUID = 1L;

    public EncryptionError(String message) {
      super(7, message, null, null);
    }
  }

  /** The document is damaged beyond recovery. */
  public static final class MalformedPdf extends PdfException {
    @Serial private static final long serialVersionUID = 1L;

    public MalformedPdf(String message) {
      super(8, message, null, null);
    }
  }

  public static final class DecodeError extends PdfException {
    @Serial private static final long serialVersionUID = 1L;

    public DecodeError(String message) {
      super(9, message, null, null);
    }
  }

  /** A cursor was cancelled while it worked. */
  public static final class Cancelled extends PdfException {
    @Serial private static final long serialVersionUID = 1L;

    public Cancelled(String message) {
      super(10, message, null, null);
    }
  }

  public static final class RuntimeError extends PdfException {
    @Serial private static final long serialVersionUID = 1L;

    public RuntimeError(String message) {
      super(11, message, null, null);
    }
  }

  /** The native library failed unexpectedly (code 12 is a caught native panic). */
  public static final class NativeError extends PdfException {
    @Serial private static final long serialVersionUID = 1L;

    public NativeError(int code, String message, Throwable cause) {
      super(code, message, null, cause);
    }
  }
}
