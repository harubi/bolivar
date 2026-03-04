from pathlib import Path

import pytest


ROOT = Path(__file__).resolve().parents[1]
PDFMINER_SAMPLE = ROOT / "references" / "pdfminer.six" / "samples" / "simple1.pdf"
PDFMINER_FILTER_SAMPLE = (
    ROOT
    / "references"
    / "pdfminer.six"
    / "samples"
    / "contrib"
    / "issue-1062-filters.pdf"
)
PDFPLUMBER_SAMPLE = (
    ROOT / "references" / "pdfplumber" / "tests" / "pdfs" / "pdffill-demo.pdf"
)

PDFMINER_DATASET_MODULES = {
    "test_converter_integration",
    "test_layout_attrs",
    "test_pdftypes_stream",
}

PDFPLUMBER_DATASET_MODULES = {
    "test_pdfplumber_async",
    "test_pdfplumber_filtered_bbox",
    "test_pdfplumber_table_parity",
    "test_pdfplumber_text_parity",
    "test_table_filtering",
}


def pytest_collection_modifyitems(
    config: pytest.Config, items: list[pytest.Item]
) -> None:
    del config

    have_pdfminer_dataset = PDFMINER_SAMPLE.exists() and PDFMINER_FILTER_SAMPLE.exists()
    have_pdfplumber_dataset = PDFPLUMBER_SAMPLE.exists()

    skip_pdfminer = pytest.mark.skip(
        reason="requires references/pdfminer.six sample PDFs"
    )
    skip_pdfplumber = pytest.mark.skip(
        reason="requires references/pdfplumber test PDFs"
    )

    for item in items:
        module_name = item.module.__name__.rsplit(".", maxsplit=1)[-1]

        if module_name in PDFMINER_DATASET_MODULES and not have_pdfminer_dataset:
            item.add_marker(skip_pdfminer)

        if module_name in PDFPLUMBER_DATASET_MODULES and not have_pdfplumber_dataset:
            item.add_marker(skip_pdfplumber)

        if (
            module_name == "test_table_api_consolidation"
            and item.name == "test_pdfplumber_async_table_extraction_still_works"
            and not have_pdfplumber_dataset
        ):
            item.add_marker(skip_pdfplumber)
