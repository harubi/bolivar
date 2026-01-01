import logging
import os
import re
import sys
import unittest
from itertools import groupby
from operator import itemgetter

import pytest

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
# Ensure vendored pdfplumber is importable
sys.path.insert(0, os.path.join(ROOT, "references/pdfplumber"))

import pdfplumber
from pdfplumber import utils

logging.disable(logging.ERROR)

HERE = os.path.join(ROOT, "references/pdfplumber/tests")


class TestTextUtilsParity(unittest.TestCase):
    @classmethod
    def setup_class(self):
        self.pdf = pdfplumber.open(os.path.join(HERE, "pdfs/pdffill-demo.pdf"))
        self.pdf_scotus = pdfplumber.open(
            os.path.join(HERE, "pdfs/scotus-transcript-p1.pdf")
        )

    @classmethod
    def teardown_class(self):
        self.pdf.close()
        self.pdf_scotus.close()

    def test_x_tolerance_ratio(self):
        pdf = pdfplumber.open(os.path.join(HERE, "pdfs/issue-987-test.pdf"))
        page = pdf.pages[0]

        assert page.extract_text() == "Big Te xt\nSmall Text"
        assert page.extract_text(x_tolerance=4) == "Big Te xt\nSmallText"
        assert page.extract_text(x_tolerance_ratio=0.15) == "Big Text\nSmall Text"

        words = page.extract_words(x_tolerance_ratio=0.15)
        assert "|".join(w["text"] for w in words) == "Big|Text|Small|Text"

    def test_extract_words(self):
        path = os.path.join(HERE, "pdfs/issue-192-example.pdf")
        with pdfplumber.open(path) as pdf:
            p = pdf.pages[0]
            words = p.extract_words(vertical_ttb=False)
            words_attr = p.extract_words(vertical_ttb=False, extra_attrs=["size"])
            words_w_spaces = p.extract_words(vertical_ttb=False, keep_blank_chars=True)
            words_rtl = p.extract_words(horizontal_ltr=False)

        assert words[0]["text"] == "Agaaaaa:"
        assert words[0]["direction"] == "ltr"

        assert "size" not in words[0]
        assert round(words_attr[0]["size"], 2) == 9.96

        assert words_w_spaces[0]["text"] == "Agaaaaa: AAAA"

        vertical = [w for w in words if w["upright"] == 0]
        assert vertical[0]["text"] == "Aaaaaabag8"
        assert vertical[0]["direction"] == "btt"

        assert words_rtl[1]["text"] == "baaabaaA/AAA"
        assert words_rtl[1]["direction"] == "rtl"

    def test_extract_words_return_chars(self):
        path = os.path.join(HERE, "pdfs/extra-attrs-example.pdf")
        with pdfplumber.open(path) as pdf:
            page = pdf.pages[0]

            words = page.extract_words()
            assert "chars" not in words[0]

            words = page.extract_words(return_chars=True)
            assert "chars" in words[0]
            assert "".join(c["text"] for c in words[0]["chars"]) == words[0]["text"]

    def test_text_rotation(self):
        rotations = {
            "0": ("ltr", "ttb"),
            "-0": ("rtl", "ttb"),
            "180": ("rtl", "btt"),
            "-180": ("ltr", "btt"),
            "90": ("ttb", "rtl"),
            "-90": ("btt", "rtl"),
            "270": ("btt", "ltr"),
            "-270": ("ttb", "ltr"),
        }

        path = os.path.join(HERE, "pdfs/issue-848.pdf")
        with pdfplumber.open(path) as pdf:
            expected = utils.text.extract_text(pdf.pages[0].chars)
            for i, (rotation, (char_dir, line_dir)) in enumerate(rotations.items()):
                if i == 0:
                    continue
                p = pdf.pages[i].filter(lambda obj: obj.get("text") != " ")
                output = utils.text.extract_text(
                    x_tolerance=2,
                    y_tolerance=2,
                    chars=p.chars,
                    char_dir=char_dir,
                    line_dir=line_dir,
                    char_dir_rotated=char_dir,
                    line_dir_rotated=line_dir,
                    char_dir_render="ltr",
                    line_dir_render="ttb",
                )
                assert output == expected

    def test_text_rotation_layout(self):
        rotations = {
            "0": ("ltr", "ttb"),
            "-0": ("rtl", "ttb"),
            "180": ("rtl", "btt"),
            "-180": ("ltr", "btt"),
            "90": ("ttb", "rtl"),
            "-90": ("btt", "rtl"),
            "270": ("btt", "ltr"),
            "-270": ("ttb", "ltr"),
        }

        def meets_expectations(text):
            a = re.search("opens with a news report", text)
            b = re.search("having been transferred", text)
            return a and b and (a.start() < b.start())

        path = os.path.join(HERE, "pdfs/issue-848.pdf")
        with pdfplumber.open(path) as pdf:
            for i, (rotation, (char_dir, line_dir)) in enumerate(rotations.items()):
                p = pdf.pages[i].filter(lambda obj: obj.get("text") != " ")
                output = p.extract_text(
                    layout=True,
                    x_tolerance=2,
                    y_tolerance=2,
                    char_dir=char_dir,
                    line_dir=line_dir,
                    char_dir_rotated=char_dir,
                    line_dir_rotated=line_dir,
                    char_dir_render="ltr",
                    line_dir_render="ttb",
                    y_density=14,
                )
                assert meets_expectations(output)

    def test_text_render_directions(self):
        path = os.path.join(HERE, "pdfs/line-char-render-example.pdf")
        targets = {
            ("ttb", "ltr"): "first line\nsecond line\nthird line",
            ("ttb", "rtl"): "enil tsrif\nenil dnoces\nenil driht",
            ("btt", "ltr"): "third line\nsecond line\nfirst line",
            ("btt", "rtl"): "enil driht\nenil dnoces\nenil tsrif",
            ("ltr", "ttb"): "fst\nieh\nrci\nsor\ntnd\n d \n l l\nili\nnin\nene\n e ",
            ("ltr", "btt"): " s \nfet\nich\nroi\nsnr\ntdd\n   \nlll\niii\nnnn\neee",
            ("rtl", "ttb"): "tsf\nhei\nicr\nros\ndnt\n d \n l l\nili\nnin\nene\n e ",
            ("rtl", "btt"): " s \ntef\nhci\nior\nrns\nddt\n   \nlll\niii\nnnn\neee",
        }
        with pdfplumber.open(path) as pdf:
            page = pdf.pages[0]
            for (line_dir, char_dir), target in targets.items():
                text = page.extract_text(
                    line_dir_render=line_dir, char_dir_render=char_dir
                )
                assert text == target

    def test_invalid_directions(self):
        path = os.path.join(HERE, "pdfs/line-char-render-example.pdf")
        pdf = pdfplumber.open(path)
        page = pdf.pages[0]
        with pytest.raises(ValueError):
            page.extract_text(line_dir="xxx", char_dir="ltr")
        with pytest.raises(ValueError):
            page.extract_text(line_dir="ttb", char_dir="a")
        with pytest.raises(ValueError):
            page.extract_text(line_dir="rtl", char_dir="ltr")
        with pytest.raises(ValueError):
            page.extract_text(line_dir="ttb", char_dir="btt")
        with pytest.raises(ValueError):
            page.extract_text(line_dir_rotated="ttb", char_dir="btt")
        with pytest.raises(ValueError):
            page.extract_text(line_dir_render="ttb", char_dir_render="btt")
        pdf.close()

    def test_extra_attrs(self):
        path = os.path.join(HERE, "pdfs/extra-attrs-example.pdf")
        with pdfplumber.open(path) as pdf:
            page = pdf.pages[0]
            assert page.extract_text() == "BlackRedArial"
            assert (
                page.extract_text(extra_attrs=["non_stroking_color"])
                == "Black RedArial"
            )
            assert page.extract_text(extra_attrs=["fontname"]) == "BlackRed Arial"
            assert (
                page.extract_text(extra_attrs=["non_stroking_color", "fontname"])
                == "Black Red Arial"
            )
            assert page.extract_text(
                layout=True,
                use_text_flow=True,
                extra_attrs=["non_stroking_color", "fontname"],
            )

    def test_extract_words_punctuation(self):
        path = os.path.join(HERE, "pdfs/test-punkt.pdf")
        with pdfplumber.open(path) as pdf:
            wordsA = pdf.pages[0].extract_words(split_at_punctuation=True)
            wordsB = pdf.pages[0].extract_words(split_at_punctuation=False)
            wordsC = pdf.pages[0].extract_words(
                split_at_punctuation=r"!\"&'()*+,.:;<=>?@[]^`{|}~"
            )

            assert wordsA[0]["text"] == "https"
            assert (
                wordsB[0]["text"]
                == "https://dell-research-harvard.github.io/HJDataset/"
            )
            assert wordsC[2]["text"] == "//dell-research-harvard"

            wordsA = pdf.pages[1].extract_words(split_at_punctuation=True)
            wordsB = pdf.pages[1].extract_words(split_at_punctuation=False)
            wordsC = pdf.pages[1].extract_words(
                split_at_punctuation=r"!\"&'()*+,.:;<=>?@[]^`{|}~"
            )

            assert len(wordsA) == 4
            assert len(wordsB) == 2
            assert len(wordsC) == 2

            wordsA = pdf.pages[2].extract_words(split_at_punctuation=True)
            wordsB = pdf.pages[2].extract_words(split_at_punctuation=False)
            wordsC = pdf.pages[2].extract_words(
                split_at_punctuation=r"!\"&'()*+,.:;<=>?@[]^`{|}~"
            )

            assert wordsA[1]["text"] == "["
            assert wordsB[1]["text"] == "[2,"
            assert wordsC[1]["text"] == "["

            wordsA = pdf.pages[3].extract_words(split_at_punctuation=True)
            wordsB = pdf.pages[3].extract_words(split_at_punctuation=False)
            wordsC = pdf.pages[3].extract_words(
                split_at_punctuation=r"!\"&'()*+,.:;<=>?@[]^`{|}~"
            )

            assert wordsA[2]["text"] == "al"
            assert wordsB[2]["text"] == "al."
            assert wordsC[2]["text"] == "al"

    def test_extract_text_punctuation(self):
        path = os.path.join(HERE, "pdfs/test-punkt.pdf")
        with pdfplumber.open(path) as pdf:
            text = pdf.pages[0].extract_text(
                layout=True,
                split_at_punctuation=True,
            )
            assert "https " in text

    def test_text_flow(self):
        path = os.path.join(HERE, "pdfs/federal-register-2020-17221.pdf")

        def words_to_text(words):
            grouped = groupby(words, key=itemgetter("top"))
            lines = [" ".join(word["text"] for word in grp) for top, grp in grouped]
            return "\n".join(lines)

        with pdfplumber.open(path) as pdf:
            p0 = pdf.pages[0]
            using_flow = p0.extract_words(use_text_flow=True)
            not_using_flow = p0.extract_words()

        target_text = (
            "The FAA proposes to\n"
            "supersede Airworthiness Directive (AD)\n"
            "2018–23–51, which applies to all The\n"
            "Boeing Company Model 737–8 and 737–\n"
            "9 (737 MAX) airplanes. Since AD 2018–\n"
        )

        assert target_text in words_to_text(using_flow)
        assert target_text not in words_to_text(not_using_flow)

    def test_text_flow_overlapping(self):
        path = os.path.join(HERE, "pdfs/issue-912.pdf")

        with pdfplumber.open(path) as pdf:
            p0 = pdf.pages[0]
            using_flow = p0.extract_text(use_text_flow=True, layout=True, x_tolerance=1)
            not_using_flow = p0.extract_text(layout=True, x_tolerance=1)

        assert re.search("2015 RICE PAYMENT 26406576 0 1207631 Cr", using_flow)
        assert re.search("124644,06155766", using_flow) is None

        assert re.search("124644,06155766", not_using_flow)
        assert (
            re.search("2015 RICE PAYMENT 26406576 0 1207631 Cr", not_using_flow) is None
        )

    def test_text_flow_words_mixed_lines(self):
        path = os.path.join(HERE, "pdfs/issue-1279-example.pdf")

        with pdfplumber.open(path) as pdf:
            p0 = pdf.pages[0]
            words = p0.extract_words(use_text_flow=True)

        texts = set(w["text"] for w in words)

        assert "claim" in texts
        assert "lence" in texts
        assert "claimlence" not in texts

    def test_extract_text(self):
        text = self.pdf.pages[0].extract_text()
        goal_lines = [
            "First Page Previous Page Next Page Last Page",
            "Print",
            "PDFill: PDF Drawing",
            "You can open a PDF or create a blank PDF by PDFill.",
            "Online Help",
            "Here are the PDF drawings created by PDFill",
            "Please save into a new PDF to see the effect!",
            "Goto Page 2: Line Tool",
            "Goto Page 3: Arrow Tool",
            "Goto Page 4: Tool for Rectangle, Square and Rounded Corner",
            "Goto Page 5: Tool for Circle, Ellipse, Arc, Pie",
            "Goto Page 6: Tool for Basic Shapes",
            "Goto Page 7: Tool for Curves",
            "Here are the tools to change line width, style, arrow style and colors",
        ]
        goal = "\n".join(goal_lines)

        assert text == goal

        text_simple = self.pdf.pages[0].extract_text_simple()
        assert text_simple == goal

        assert self.pdf.pages[0].crop((0, 0, 1, 1)).extract_text() == ""

    def test_extract_text_layout(self):
        target = (
            open(os.path.join(HERE, "comparisons/scotus-transcript-p1.txt"))
            .read()
            .strip("\n")
        )
        page = self.pdf_scotus.pages[0]
        text = page.extract_text(layout=True)
        utils_text = utils.extract_text(
            page.chars,
            layout=True,
            layout_width=page.width,
            layout_height=page.height,
            layout_bbox=page.bbox,
        )
        assert text == utils_text
        assert text == target

    def test_extract_text_layout_cropped(self):
        target = (
            open(os.path.join(HERE, "comparisons/scotus-transcript-p1-cropped.txt"))
            .read()
            .strip("\n")
        )
        p = self.pdf_scotus.pages[0]
        cropped = p.crop((90, 70, p.width, 300))
        text = cropped.extract_text(layout=True)
        assert text == target

    def test_extract_text_layout_widths(self):
        p = self.pdf_scotus.pages[0]
        text = p.extract_text(layout=True, layout_width_chars=75)
        assert all(len(line) == 75 for line in text.splitlines())
        with pytest.raises(ValueError):
            p.extract_text(layout=True, layout_width=300, layout_width_chars=50)
        with pytest.raises(ValueError):
            p.extract_text(layout=True, layout_height=300, layout_height_chars=50)

    def test_extract_text_nochars(self):
        charless = self.pdf.pages[0].filter(lambda df: df["object_type"] != "char")
        assert charless.extract_text() == ""
        assert charless.extract_text(layout=True) == ""

    def test_extract_text_lines(self):
        page = self.pdf_scotus.pages[0]
        results = page.extract_text_lines()
        assert len(results) == 28
        assert "chars" in results[0]
        assert results[0]["text"] == "Official - Subject to Final Review"

        alt = page.extract_text_lines(layout=True, strip=False, return_chars=False)
        assert "chars" not in alt[0]
        assert (
            alt[0]["text"]
            == "                                   Official - Subject to Final Review               "
        )

        assert results[10]["text"] == "10 Tuesday, January 13, 2009"
        assert (
            alt[10]["text"]
            == "            10                          Tuesday, January 13, 2009                   "
        )
        assert (
            page.extract_text_lines(layout=True)[10]["text"]
            == "10                          Tuesday, January 13, 2009"
        )


class TestDedupeCharsParity(unittest.TestCase):
    @classmethod
    def setup_class(self):
        path = os.path.join(HERE, "pdfs/issue-71-duplicate-chars.pdf")
        self.pdf = pdfplumber.open(path)

    @classmethod
    def teardown_class(self):
        self.pdf.close()

    def test_extract_table(self):
        page = self.pdf.pages[0]
        table_without_drop_duplicates = page.extract_table()
        table_with_drop_duplicates = page.dedupe_chars().extract_table()
        last_line_without_drop = table_without_drop_duplicates[1][1].split("\n")[-1]
        last_line_with_drop = table_with_drop_duplicates[1][1].split("\n")[-1]

        assert (
            last_line_without_drop
            == "微微软软 培培训训课课程程：： 名名模模意意义义一一些些有有意意义义一一些些"
        )
        assert last_line_with_drop == "微软 培训课程： 名模意义一些有意义一些"

    def test_extract_words(self):
        page = self.pdf.pages[0]
        x0 = 440.143
        x1_without_drop = 534.992
        x1_with_drop = 534.719
        top_windows = 791.849
        top_linux = 794.357
        bottom = 802.961
        last_words_without_drop = page.extract_words()[-1]
        last_words_with_drop = page.dedupe_chars().extract_words()[-1]

        assert round(last_words_without_drop["x0"], 3) == x0
        assert round(last_words_without_drop["x1"], 3) == x1_without_drop
        assert round(last_words_without_drop["top"], 3) in (top_windows, top_linux)
        assert round(last_words_without_drop["bottom"], 3) == bottom
        assert last_words_without_drop["upright"] == 1
        assert (
            last_words_without_drop["text"]
            == "名名模模意意义义一一些些有有意意义义一一些些"
        )

        assert round(last_words_with_drop["x0"], 3) == x0
        assert round(last_words_with_drop["x1"], 3) == x1_with_drop
        assert round(last_words_with_drop["top"], 3) in (top_windows, top_linux)
        assert round(last_words_with_drop["bottom"], 3) == bottom
        assert last_words_with_drop["upright"] == 1
        assert last_words_with_drop["text"] == "名模意义一些有意义一些"

    def test_extract_text(self):
        page = self.pdf.pages[0]
        last_line_without_drop = page.extract_text().split("\n")[-1]
        last_line_with_drop = page.dedupe_chars().extract_text().split("\n")[-1]

        assert (
            last_line_without_drop
            == "微微软软 培培训训课课程程：： 名名模模意意义义一一些些有有意意义义一一些些"
        )
        assert last_line_with_drop == "微软 培训课程： 名模意义一些有意义一些"

    def test_extract_text2(self):
        path = os.path.join(HERE, "pdfs/issue-71-duplicate-chars-2.pdf")
        pdf = pdfplumber.open(path)
        page = pdf.pages[0]

        assert (
            page.dedupe_chars().extract_text(y_tolerance=6).splitlines()[4]
            == "UE 8. Circulation - Métabolismes"
        )

    def test_extra_attrs(self):
        path = os.path.join(HERE, "pdfs/issue-1114-dedupe-chars.pdf")
        pdf = pdfplumber.open(path)
        page = pdf.pages[0]

        def dup_chars(s: str) -> str:
            return "".join((char if char == " " else char + char) for char in s)

        ground_truth = (
            ("Simple", False, False),
            ("Duplicated", True, True),
            ("Font", "fontname", True),
            ("Size", "size", True),
            ("Italic", "fontname", True),
            ("Weight", "fontname", True),
            ("Horizontal shift", False, "HHoorrizizoonntatal ls shhifitft"),
            ("Vertical shift", False, True),
        )
        gt = []
        for text, should_dedup, dup_text in ground_truth:
            if isinstance(dup_text, bool):
                if dup_text:
                    dup_text = dup_chars(text)
                else:
                    dup_text = text
            gt.append((text, should_dedup, dup_text))

        keys_list = ["no_dedupe", (), ("size",), ("fontname",), ("size", "fontname")]
        for keys in keys_list:
            if keys != "no_dedupe":
                filtered_page = page.dedupe_chars(tolerance=2, extra_attrs=keys)
            else:
                filtered_page = page
            for i, line in enumerate(
                filtered_page.extract_text(y_tolerance=5).splitlines()
            ):
                text, should_dedup, dup_text = gt[i]
                if keys == "no_dedupe":
                    should_dedup = False
                if isinstance(should_dedup, str):
                    if should_dedup in keys:
                        assert line == dup_text
                    else:
                        assert line == text
                elif should_dedup:
                    assert line == text
                else:
                    assert line == dup_text


class TestIssuesTextParity(unittest.TestCase):
    def test_pr_88_extract_words_count(self):
        path = os.path.join(HERE, "pdfs/pr-88-example.pdf")
        with pdfplumber.open(path) as pdf:
            page = pdf.pages[0]
            words = page.extract_words()
            assert len(words) == 25

    def test_issue_90_extract_words_noerror(self):
        path = os.path.join(HERE, "pdfs/issue-90-example.pdf")
        with pdfplumber.open(path) as pdf:
            page = pdf.pages[0]
            page.extract_words()

    def test_pr_136_extract_words_noerror(self):
        path = os.path.join(HERE, "pdfs/pr-136-example.pdf")
        with pdfplumber.open(path) as pdf:
            page = pdf.pages[0]
            page.extract_words()

    def test_pr_138_extract_tables_explicit_lines(self):
        path = os.path.join(HERE, "pdfs/pr-138-example.pdf")
        with pdfplumber.open(path) as pdf:
            page = pdf.pages[0]
            assert len(page.chars) == 5140
            page.extract_tables(
                {
                    "vertical_strategy": "explicit",
                    "horizontal_strategy": "lines",
                    "explicit_vertical_lines": page.curves + page.edges,
                }
            )

    def test_issue_216_extract_table_none(self):
        path = os.path.join(HERE, "pdfs/issue-140-example.pdf")
        with pdfplumber.open(path) as pdf:
            cropped = pdf.pages[0].crop((0, 0, 1, 1))
            assert cropped.extract_table() is None

    def test_issue_386_extract_text_iterator(self):
        path = os.path.join(HERE, "pdfs/nics-background-checks-2015-11.pdf")
        with pdfplumber.open(path) as pdf:
            chars = (char for char in pdf.chars)
            pdfplumber.utils.extract_text(chars)

    def test_issue_271_use_text_flow(self):
        path = os.path.join(HERE, "pdfs/issue-1279-example.pdf")
        with pdfplumber.open(path) as pdf:
            page = pdf.pages[0]
            text = page.extract_text(use_text_flow=True)
            words = " ".join(w["text"] for w in page.extract_words(use_text_flow=True))
            assert text
            assert words


class TestNicsReportTextParity(unittest.TestCase):
    def test_nics_extract_text(self):
        path = os.path.join(HERE, "pdfs/nics-background-checks-2015-11.pdf")
        with pdfplumber.open(path) as pdf:
            month_chars = pdf.pages[0].chars[:50]
            month_text = utils.extract_text(month_chars)
            assert month_text

    def test_nics_extract_text_filtered(self):
        path = os.path.join(HERE, "pdfs/nics-background-checks-2015-11.pdf")
        with pdfplumber.open(path) as pdf:
            filtered = pdf.pages[0].filter(lambda obj: obj.get("text") == "Alabama")
            text = filtered.extract_text()
            assert "Alabama" in text

    def test_nics_find_tables(self):
        path = os.path.join(HERE, "pdfs/nics-background-checks-2015-11.pdf")
        with pdfplumber.open(path) as pdf:
            cropped = pdf.pages[0].crop((0, 0, 580, 500))
            tables = cropped.find_tables(
                {
                    "vertical_strategy": "lines",
                    "horizontal_strategy": "lines",
                }
            )
            assert tables
