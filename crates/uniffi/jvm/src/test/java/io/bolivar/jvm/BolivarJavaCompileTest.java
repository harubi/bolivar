package io.bolivar.jvm;

import static org.junit.jupiter.api.Assertions.assertEquals;

import java.lang.reflect.Method;
import java.util.concurrent.CompletableFuture;
import org.junit.jupiter.api.Test;

class BolivarJavaCompileTest {
  @Test
  void asyncWrapperReturnsCompletableFuture() throws Exception {
    Method method = BolivarAsync.class.getMethod("extractTextFromPathAsync", String.class, String.class);
    assertEquals(CompletableFuture.class, method.getReturnType());
  }

  @Test
  void syncWrapperHasExpectedReturnType() throws Exception {
    Method method = Bolivar.class.getMethod("extractTextFromPath", String.class, String.class);
    assertEquals(String.class, method.getReturnType());
  }
}
