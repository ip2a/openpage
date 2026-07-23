import unittest

import openpage


class FacadeTests(unittest.TestCase):
    def test_public_surface_is_core_facade(self) -> None:
        self.assertEqual(openpage.__all__, ["Browser", "Page", "Session", "open"])
        self.assertEqual(openpage.Browser.__name__, "Browser")
        self.assertEqual(openpage.Page.__name__, "Page")
        self.assertEqual(openpage.Session.__name__, "Session")

    def test_session_is_constructible(self) -> None:
        self.assertIsInstance(openpage.Session(), openpage.Session)


if __name__ == "__main__":
    unittest.main()
