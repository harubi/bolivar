package sa.ingenious.bolivar

import java.util.concurrent.CompletionException
import java.util.concurrent.ExecutionException
import sa.ingenious.bolivar.ffi.BolivarException as NativeBolivarException

sealed class BolivarException(
    message: String,
    cause: Throwable? = null,
) : RuntimeException(message, cause) {
    class InvalidPath(
        message: String,
        cause: Throwable? = null,
    ) : BolivarException(message, cause)

    class InvalidArgument(
        message: String,
        cause: Throwable? = null,
    ) : BolivarException(message, cause)

    class IoNotFound(
        message: String,
        cause: Throwable? = null,
    ) : BolivarException(message, cause)

    class IoPermissionDenied(
        message: String,
        cause: Throwable? = null,
    ) : BolivarException(message, cause)

    class IoError(
        message: String,
        cause: Throwable? = null,
    ) : BolivarException(message, cause)

    class SyntaxError(
        message: String,
        cause: Throwable? = null,
    ) : BolivarException(message, cause)

    class EncryptionError(
        message: String,
        cause: Throwable? = null,
    ) : BolivarException(message, cause)

    class PdfError(
        message: String,
        cause: Throwable? = null,
    ) : BolivarException(message, cause)

    class DecodeError(
        message: String,
        cause: Throwable? = null,
    ) : BolivarException(message, cause)

    class RuntimeError(
        message: String,
        cause: Throwable? = null,
    ) : BolivarException(message, cause)

    class NativeError(
        message: String,
        cause: Throwable? = null,
    ) : BolivarException(message, cause)

    companion object {
        @JvmStatic
        fun from(throwable: Throwable): BolivarException {
            if (throwable is BolivarException) {
                return throwable
            }

            val root = unwrapThrowable(throwable)
            if (root is BolivarException) {
                return root
            }

            if (root is NativeBolivarException) {
                return fromNative(root)
            }

            val message = root.message ?: root.javaClass.simpleName
            return NativeError(message, root)
        }

        private fun fromNative(error: NativeBolivarException): BolivarException =
            when (error) {
                is NativeBolivarException.InvalidPath -> {
                    InvalidPath(error.message ?: "Invalid path", error)
                }

                is NativeBolivarException.InvalidArgument -> {
                    InvalidArgument(error.message ?: "Invalid argument", error)
                }

                is NativeBolivarException.IoNotFound -> {
                    IoNotFound(error.message ?: "File not found", error)
                }

                is NativeBolivarException.IoPermissionDenied -> {
                    IoPermissionDenied(error.message ?: "Permission denied", error)
                }

                is NativeBolivarException.IoException -> {
                    IoError(error.message ?: "IO error", error)
                }

                is NativeBolivarException.SyntaxException -> {
                    SyntaxError(error.message ?: "Syntax error", error)
                }

                is NativeBolivarException.EncryptionException -> {
                    EncryptionError(error.message ?: "Encryption error", error)
                }

                is NativeBolivarException.PdfException -> {
                    PdfError(error.message ?: "PDF error", error)
                }

                is NativeBolivarException.DecodeException -> {
                    DecodeError(error.message ?: "Decode error", error)
                }

                is NativeBolivarException.RuntimeException -> {
                    RuntimeError(error.message ?: "Runtime error", error)
                }
            }
    }
}

internal inline fun <T> withTranslatedExceptions(block: () -> T): T =
    try {
        block()
    } catch (throwable: Throwable) {
        throw BolivarException.from(throwable)
    }

private fun unwrapThrowable(throwable: Throwable): Throwable {
    var current = throwable
    while (current is CompletionException || current is ExecutionException) {
        val cause = current.cause ?: break
        current = cause
    }
    return current
}
