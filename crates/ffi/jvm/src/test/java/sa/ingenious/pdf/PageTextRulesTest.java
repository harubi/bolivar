package sa.ingenious.pdf;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.util.ArrayList;
import java.util.List;
import org.junit.jupiter.api.Test;

/** The glyph summary rules, as the old Rust binding applied them. */
class PageTextRulesTest {
  @Test
  void digitsAreUnicodeDecimalDigits() {
    for (char digit = '٠'; digit <= '٩'; digit++) {
      assertTrue(Decode.hasDecimalDigit(String.valueOf(digit)));
    }
    assertTrue(Decode.hasDecimalDigit("۵"));
    assertTrue(Decode.hasDecimalDigit("a7"));
    assertTrue(Decode.hasDecimalDigit("ب٣"));
    assertTrue(Decode.hasDecimalDigit("𝟎"), "mathematical bold zero is Nd");
    assertFalse(Decode.hasDecimalDigit("abc"));
    assertFalse(Decode.hasDecimalDigit("½"), "a vulgar fraction is No");
    assertFalse(Decode.hasDecimalDigit("Ⅳ"), "a roman numeral is Nl");
  }

  @Test
  void blankFollowsJavaWhitespace() {
    for (String blank : List.of("", " ", "\t", "\u001C", " ", " ", " ", "　")) {
      assertTrue(blank.isBlank(), blank);
    }
    for (String text : List.of(" ", " ", " ", "\u0085", "​", "a")) {
      assertFalse(text.isBlank(), text);
    }
  }

  @Test
  void sizesRoundHalfUpAndSaturate() {
    assertEquals(9, Decode.wholeSize(8.5));
    assertEquals(0, Decode.wholeSize(-0.5));
    assertEquals(-1, Decode.wholeSize(-1.5));
    assertEquals(-2, Decode.wholeSize(-1.6));
    assertEquals(0, Decode.wholeSize(Double.NaN));
    assertEquals(Integer.MAX_VALUE, Decode.wholeSize(1e12));
    assertEquals(Integer.MIN_VALUE, Decode.wholeSize(-1e12));
  }

  @Test
  void namesSortByCodePointLikeRust() {
    List<String> names = new ArrayList<>(List.of("�", "😀", "B", "A", "AB"));
    names.sort(Decode.CODE_POINT_ORDER);
    // UTF-16 order would put the surrogate pair before U+FFFD.
    assertEquals(List.of("A", "AB", "B", "�", "😀"), names);
  }

  @Test
  void pageTextsSummariseFontsAndDigits() {
    try (var document =
            Document.open(Fixtures.path("pdfplumber/nics-background-checks-2015-11.pdf"));
        var texts = document.pageTexts()) {
      PageText page = texts.next();
      assertFalse(page.fonts().isEmpty());
      assertFalse(page.digitStyles().isEmpty());
      List<String> sorted = new ArrayList<>(page.fonts());
      sorted.sort(Decode.CODE_POINT_ORDER);
      assertEquals(sorted, page.fonts());
    }
  }
}
