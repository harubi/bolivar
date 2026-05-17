package sa.ingenious.pdf

import java.util.concurrent.CompletionException
import java.util.concurrent.ExecutionException
import sa.ingenious.ffi.BolivarException as NativePdfException

sealed class PdfException(
    message: String,
    cause: Throwable? = null,
) : RuntimeException(message, cause) {
    class InvalidPath(
        message: String,
        cause: Throwable? = null,
    ) : PdfException(message, cause)

    class InvalidArgument(
        message: String,
        cause: Throwable? = null,
    ) : PdfException(message, cause)

    class IoNotFound(
        message: String,
        cause: Throwable? = null,
    ) : PdfException(message, cause)

    class IoPermissionDenied(
        message: String,
        cause: Throwable? = null,
    ) : PdfException(message, cause)

    class IoError(
        message: String,
        cause: Throwable? = null,
    ) : PdfException(message, cause)

    class SyntaxError(
        message: String,
        cause: Throwable? = null,
    ) : PdfException(message, cause)

    class EncryptionError(
        message: String,
        cause: Throwable? = null,
    ) : PdfException(message, cause)

    class MalformedPdf(
        message: String,
        cause: Throwable? = null,
    ) : PdfException(message, cause)

    class DecodeError(
        message: String,
        cause: Throwable? = null,
    ) : PdfException(message, cause)

    class RuntimeError(
        message: String,
        cause: Throwable? = null,
    ) : PdfException(message, cause)

    class NativeError(
        message: String,
        cause: Throwable? = null,
    ) : PdfException(message, cause)

    companion object {
        @JvmStatic
        fun from(throwable: Throwable): PdfException {
            if (throwable is PdfException) {
                return throwable
            }

            val root = unwrapThrowable(throwable)
            if (root is PdfException) {
                return root
            }

            if (root is NativePdfException) {
                return fromNative(root)
            }

            val message = root.message ?: root.javaClass.simpleName
            return NativeError(message, root)
        }

        private fun fromNative(error: NativePdfException): PdfException =
            when (error) {
                is NativePdfException.InvalidPath -> InvalidPath(error.message ?: "Invalid path", error)
                is NativePdfException.InvalidArgument -> InvalidArgument(error.message ?: "Invalid argument", error)
                is NativePdfException.IoNotFound -> IoNotFound(error.message ?: "File not found", error)
                is NativePdfException.IoPermissionDenied -> IoPermissionDenied(error.message ?: "Permission denied", error)
                is NativePdfException.IoException -> IoError(error.message ?: "IO error", error)
                is NativePdfException.SyntaxException -> SyntaxError(error.message ?: "Syntax error", error)
                is NativePdfException.EncryptionException -> EncryptionError(error.message ?: "Encryption error", error)
                is NativePdfException.PdfException -> MalformedPdf(error.message ?: "PDF error", error)
                is NativePdfException.DecodeException -> DecodeError(error.message ?: "Decode error", error)
                is NativePdfException.RuntimeException -> RuntimeError(error.message ?: "Runtime error", error)
            }
    }
}

internal inline fun <T> withPdfExceptions(block: () -> T): T =
    try {
        block()
    } catch (throwable: Throwable) {
        throw PdfException.from(throwable)
    }

private fun unwrapThrowable(throwable: Throwable): Throwable {
    var current = throwable
    while (current is CompletionException || current is ExecutionException) {
        val cause = current.cause ?: break
        current = cause
    }
    return current
}
