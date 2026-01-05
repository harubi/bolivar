import bolivar


def test_extract_text_bytes(benchmark, text_fixture):
    _, data = text_fixture
    result = benchmark(lambda: bolivar.extract_text(data, threads=1))
    assert result is not None
