import bolivar._bridge_api as _bridge_api


def test_bridge_api_does_not_expose_dead_table_stream():
    assert not hasattr(_bridge_api, "_extract_tables_stream")
