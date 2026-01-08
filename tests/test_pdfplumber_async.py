import pdfplumber
import asyncio


def test_pdfplumber_pages_async_iterates():
    path = "references/pdfplumber/tests/pdfs/nics-background-checks-2015-11.pdf"
    async def run():
        async with pdfplumber.open(path) as pdf:
            pages = []
            async for page in pdf.pages:
                pages.append(page)
            assert pages
            assert pages[0].extract_text()
    asyncio.run(run())


def test_pdfplumber_pages_async_without_context():
    path = "references/pdfplumber/tests/pdfs/nics-background-checks-2015-11.pdf"
    async def run():
        pdf = pdfplumber.open(path)
        pages = []
        async for page in pdf.pages:
            pages.append(page)
            if len(pages) >= 2:
                break
        assert pages
        pdf.close()
    asyncio.run(run())
