"""Ordered admission controls: stalled service cannot stop scheduled arrivals."""

import queue
import threading
import unittest
from concurrent.futures import ThreadPoolExecutor
from contextlib import nullcontext
from unittest.mock import patch

from application_offered import Slots, client, dispatch


class Clock:
    def __init__(self):
        self.value = 100
        self.pauses = []

    def now(self):
        return self.value

    def pause(self, seconds):
        self.pauses.append(seconds)
        self.value += round(seconds * 1_000_000_000)


class OfferedTests(unittest.TestCase):
    def test_invalid_slot_capacity_cannot_create_an_unbounded_queue(self):
        for capacity in [0, -1, 257, 1.5]:
            with self.assertRaises(AssertionError):
                Slots(capacity)

    def test_held_client_drops_arrivals_without_waiting_for_completion(self):
        slots, clock = Slots(1), Clock()
        records, origin = dispatch(slots, 1000, 4, threading.Event(), clock.now, clock.pause)
        self.assertEqual(origin, 100)
        self.assertEqual([r['scheduled_ns'] for r in records], [0, 1000000, 2000000, 3000000])
        self.assertEqual([r['offered_ns'] for r in records], [0, 1000000, 2000000, 3000000])
        self.assertEqual([r['client'] for r in records], [0, None, None, None])
        self.assertEqual(slots.jobs[0].get_nowait(), (records[0], origin))
        self.assertEqual(slots.jobs[0].maxsize, 1)
        self.assertEqual(slots.free.maxsize, 1)
        with self.assertRaises(queue.Empty):
            slots.free.get_nowait()

    def test_capacity_is_available_only_after_explicit_return(self):
        slots = Slots(2)
        first, second, third = {}, {}, {}
        slots.offer(first, 7)
        slots.offer(second, 7)
        slots.offer(third, 7)
        self.assertEqual([r['client'] for r in [first, second, third]], [0, 1, None])
        self.assertEqual(slots.jobs[0].get_nowait(), (first, 7))
        still_full = {}
        slots.offer(still_full, 7)
        self.assertIsNone(still_full['client'])
        slots.free.put_nowait(0)
        next_request = {}
        slots.offer(next_request, 7)
        self.assertEqual(next_request['client'], 0)

    def test_late_dispatch_retains_original_schedule(self):
        clock, slots = Clock(), Slots(1)

        def delayed(seconds):
            clock.pause(seconds)
            clock.value += 2500000

        records, _ = dispatch(slots, 1000, 4, threading.Event(), clock.now, delayed)
        self.assertEqual([r['scheduled_ns'] for r in records], [0, 1000000, 2000000, 3000000])
        self.assertEqual([r['offered_ns'] for r in records], [0, 3500000, 3500000, 3500000])
        self.assertEqual(len(clock.pauses), 1)

    def test_fixed_point_rate_does_not_accumulate_rounding_error(self):
        clock = Clock()
        records, _ = dispatch(Slots(1), 3, 4, threading.Event(), clock.now, clock.pause)
        self.assertEqual([r['scheduled_ns'] for r in records], [0, 333333333, 666666666, 1000000000])

    def test_worker_failure_stops_dispatch_instead_of_becoming_a_drop(self):
        stopped = threading.Event()
        stopped.set()
        with self.assertRaisesRegex(AssertionError, 'client failed'):
            dispatch(Slots(1), 1000, 4, stopped)

    def test_real_worker_held_in_exchange_does_not_suppress_offers(self):
        class Server:
            def connect(self):
                return nullcontext(None)

        entered, release, stopped = threading.Event(), threading.Event(), threading.Event()
        slots, ready, reports, clock = Slots(1), threading.Barrier(2, timeout=5), [None], Clock()

        def exchange(_stream, sequence):
            entered.set()
            self.assertTrue(release.wait(5), 'held client was not released')
            return 1, bytes([sequence])

        def pause(seconds):
            self.assertTrue(entered.wait(5), 'worker did not enter the held exchange')
            clock.pause(seconds)

        with patch('application_offered.exchange', exchange), ThreadPoolExecutor(max_workers=1) as workers:
            future = workers.submit(client, Server(), 0, slots, ready, stopped, reports)
            try:
                ready.wait()
                records, _ = dispatch(slots, 1000, 4, stopped, clock.now, pause)
                self.assertFalse(release.is_set())
                self.assertEqual([r['client'] for r in records], [0, None, None, None])
                self.assertNotIn('completed_ns', records[0])
            finally:
                stopped.set()
                release.set()
            self.assertEqual(future.result(timeout=5)['sequences'], [0])
        self.assertEqual(slots.free.get_nowait(), 0)


if __name__ == '__main__':
    unittest.main()
