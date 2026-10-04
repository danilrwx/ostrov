#!/usr/bin/env python3
"""ostrov's launcher mode for Google: g words, one row opening the search."""

import os
import sys
import urllib.parse

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from ostrov_plugin import Plugin  # noqa: E402


class Google(Plugin):
    def query(self, mode, text):
        words = text.strip()
        if not words:
            return []
        q = urllib.parse.quote(words, safe="")
        return [{"text": f"Search Google for {words}", "open": f"https://www.google.com/search?q={q}"}]


if __name__ == "__main__":
    Google().main()
