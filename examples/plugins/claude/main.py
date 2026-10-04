#!/usr/bin/env python3
"""ostrov's launcher mode for Claude: ?question opens claude.ai with the question asked; so does
`ostrov plugin claude ask QUESTION`."""

import os
import subprocess
import sys
import urllib.parse

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from ostrov_plugin import OstrovError, Plugin  # noqa: E402


def url(question):
    """Claude's page with the question typed in and sent."""
    return "https://claude.ai/new?q=" + urllib.parse.quote(question, safe="")


class Claude(Plugin):
    def query(self, mode, text):
        question = text.strip()
        if not question:
            return []
        # ostrov opens it when picked: no pick needed
        return [{"id": "ask", "text": f"Ask Claude: {question}", "icon": "dialog-question-symbolic",
                 "open": url(question)}]

    def run(self, args, input=None):
        match args:
            case ["ask", *words] if words:
                # nothing of the browser's on stdout, the protocol's
                subprocess.Popen(["xdg-open", url(" ".join(words))], stdout=subprocess.DEVNULL,
                                 stderr=subprocess.DEVNULL, start_new_session=True)
                return ""
        raise OstrovError("usage: ask QUESTION...")


if __name__ == "__main__":
    Claude().main()
