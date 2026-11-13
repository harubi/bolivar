#!/usr/bin/env python
import argparse
import json
import sys
from collections import defaultdict, deque
from itertools import chain
from typing import Any, BinaryIO, TextIO, cast

from .pdf import PDF

if len(sys.argv) == 1:
    sys.argv.append("--help")


def parse_page_spec(p_str: str) -> list[int]:
    if "-" in p_str:
        start, end = map(int, p_str.split("-"))
        return list(range(start, end + 1))
    else:
        return [int(p_str)]


def parse_args(args_raw: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser("pdfplumber")

    parser.add_argument("infile", nargs="?")
    group = parser.add_mutually_exclusive_group()
    group.add_argument(
        "--structure",
        help="Write the structure tree as JSON.  "
        "All other arguments except --pages, --laparams, and --indent will be ignored",
        action="store_true",
    )
    group.add_argument(
        "--structure-text",
        help="Write the structure tree as JSON including text contents.  "
        "All other arguments except --pages, --laparams, and --indent will be ignored",
        action="store_true",
    )

    parser.add_argument("--format", choices=["csv", "json", "text"], default="csv")

    parser.add_argument("--types", nargs="+")

    parser.add_argument(
        "--include-attrs",
        nargs="+",
        help="Include *only* these object attributes in output.",
    )

    parser.add_argument(
        "--exclude-attrs",
        nargs="+",
        help="Exclude these object attributes from output.",
    )

    parser.add_argument("--laparams", type=json.loads)

    parser.add_argument("--precision", type=int)

    parser.add_argument("--pages", nargs="+", type=parse_page_spec)

    parser.add_argument(
        "--indent", type=int, help="Indent level for JSON pretty-printing."
    )

    args = parser.parse_args(args_raw)
    args.infile = _open_infile(parser, args.infile)
    if args.pages is not None:
        args.pages = list(chain(*args.pages))
    return args


def _open_infile(parser: argparse.ArgumentParser, path: str | None) -> BinaryIO | None:
    """Open *path* the way ``argparse.FileType("rb")`` did, after parsing."""
    if path is None:
        return None
    if path == "-":
        return cast("TextIO", sys.stdin).buffer
    try:
        return open(path, "rb")
    except OSError as e:
        parser.error(f"argument infile: can't open '{path}': {e}")


def add_text_to_mcids(pdf: PDF, data: list[dict[str, Any]]) -> None:
    page_contents: defaultdict[int, Any] = defaultdict(lambda: defaultdict(str))
    for page in pdf.pages:
        text_contents = page_contents[page.page_number]
        for c in page.chars:
            mcid = c.get("mcid")
            if mcid is None:
                continue
            text_contents[mcid] += c["text"]
    d = deque(data)
    while d:
        el = d.popleft()
        if "children" in el:
            d.extend(el["children"])
        pageno = el.get("page_number")
        if pageno is None:
            continue
        text_contents = page_contents[pageno]
        if "mcids" in el:
            el["text"] = [text_contents[mcid] for mcid in el["mcids"]]


def main(args_raw: list[str] = sys.argv[1:]) -> None:
    args = parse_args(args_raw)
    try:
        with PDF.open(args.infile, pages=args.pages, laparams=args.laparams) as pdf:
            if args.structure:
                print(json.dumps(pdf.structure_tree, indent=args.indent))
            elif args.structure_text:
                tree = [cast("dict[str, Any]", el) for el in pdf.structure_tree]
                add_text_to_mcids(pdf, tree)
                print(json.dumps(tree, indent=args.indent, ensure_ascii=False))
            elif args.format == "csv":
                pdf.to_csv(
                    sys.stdout,
                    args.types,
                    precision=args.precision,
                    include_attrs=args.include_attrs,
                    exclude_attrs=args.exclude_attrs,
                )
            elif args.format == "text":
                for page in pdf.pages:
                    print(page.extract_text(layout=True))
            else:
                pdf.to_json(
                    sys.stdout,
                    args.types,
                    precision=args.precision,
                    include_attrs=args.include_attrs,
                    exclude_attrs=args.exclude_attrs,
                    indent=args.indent,
                )
    finally:
        # argparse.FileType never closed the file; close it unless it is stdin.
        stdin = getattr(sys.stdin, "buffer", None)
        if args.infile is not None and args.infile is not stdin:
            args.infile.close()


if __name__ == "__main__":
    main()
