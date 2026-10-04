#!/usr/bin/env python3
"""ostrov's example calendar: one event today at noon, for the calendar's month and its Coming Up."""

import datetime
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from ostrov_plugin import Plugin  # noqa: E402


class Demo(Plugin):
    def calendar_events(self, start, end):
        # start and end are local ISO times, the span ostrov shows; the events' times are local ISO too
        noon = datetime.datetime.combine(datetime.date.today(), datetime.time(12))
        if not start <= noon.isoformat() < end:
            return []
        return [{
            "title": "Lunch with ostrov",
            "start": noon.isoformat(),
            "end": (noon + datetime.timedelta(hours=1)).isoformat(),
            "all_day": False,
            "location": "the kitchen",
            "color": "#e5a50a",
        }]


Demo().main()
