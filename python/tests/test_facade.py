def test_public_facade():
    import openpage

    assert openpage.__all__ == ["Browser", "Page", "Session", "open"]
    assert openpage.Session().__class__.__name__ == "Session"
