"""Fixed offered arrivals over bounded persistent clients, never completion paced."""

import concurrent.futures
import hashlib
import json
import queue
import threading
import time

from application_client import Service, exchange
import application_offered_verify as verify
import evidence


class Slots:
    def __init__(self, concurrency):
        assert isinstance(concurrency, int) and 1 <= concurrency <= 256
        self.free = queue.Queue(maxsize=concurrency)
        self.jobs = [queue.Queue(maxsize=1) for _ in range(concurrency)]
        self.records = []
        for index in range(concurrency):
            self.free.put_nowait(index)

    def offer(self, record, origin, now=time.perf_counter_ns):
        try:
            index = self.free.get_nowait()
        except queue.Empty:
            record['client'] = None
            return
        record['client'] = index
        record['admitted_ns'] = now() - origin
        self.jobs[index].put_nowait((record, origin))


def dispatch(slots, rate, count, stopped, now=time.perf_counter_ns, pause=time.sleep):
    verify.bounds([rate], count)
    origin = now()
    for sequence in range(count):
        scheduled = sequence * 1_000_000_000 // rate
        while (remaining := origin + scheduled - now()) > 0:
            pause(remaining / 1_000_000_000)
        assert not stopped.is_set(), 'offered-load client failed; do not count errors as drops'
        record = dict(sequence=sequence, scheduled_ns=scheduled, offered_ns=now() - origin)
        slots.records.append(record)
        slots.offer(record, origin, now)
    return slots.records, origin


def client(server, index, slots, ready, stopped, reports):
    digest = hashlib.sha256()
    report = dict(client=index, sequences=[])
    reports[index] = report
    try:
        admitted = time.perf_counter_ns()
        with server.connect() as stream:
            report['admission_ns'] = time.perf_counter_ns() - admitted
            ready.wait()
            while True:
                try:
                    # On normal stop, drain the one already accepted slot first.
                    record, origin = slots.jobs[index].get(timeout=0.05)
                except queue.Empty:
                    if stopped.is_set():
                        break
                    continue
                record['started_ns'] = time.perf_counter_ns() - origin
                elapsed, response = exchange(stream, record['sequence'])
                record['completed_ns'] = time.perf_counter_ns() - origin
                record['exchange_ns'] = elapsed
                digest.update(response)
                report['sequences'].append(record['sequence'])
                # Return capacity only after receipt, payload validation and recording.
                slots.free.put_nowait(index)
        report['response_sha256'] = digest.hexdigest()
        return report
    except BaseException as error:
        report['error'] = f'{type(error).__name__}: {error}'
        stopped.set()
        ready.abort()
        raise


def run(binary, out, carriers, concurrency, rate, count):
    verify.bounds([rate], count)
    slots, stopped = Slots(concurrency), threading.Event()
    clients = [None] * concurrency
    ready = threading.Barrier(concurrency + 1, timeout=15)
    report = dict(schema=1, kind='offered-load', status='failed', carriers=carriers,
                  concurrency=concurrency, rate=rate, count=count, model='fixed-offered-drop-if-busy',
                  input_bytes=64, output_bytes=256, clients=clients, records=slots.records)
    try:
        with Service(binary, out, carriers, concurrency, concurrency, 10_000) as server:
            with concurrent.futures.ThreadPoolExecutor(max_workers=concurrency) as workers:
                futures = [workers.submit(client, server, index, slots, ready, stopped, clients)
                           for index in range(concurrency)]
                try:
                    ready.wait()
                    report['parked'] = server.until(lambda state: state['active'] == concurrency
                                                   and state['io_readers'] == concurrency)
                    report['parked_resource'] = evidence.resources(server.process.pid, server.start)
                    records, origin = dispatch(slots, rate, count, stopped)
                    report['offer_elapsed_ns'] = time.perf_counter_ns() - origin
                finally:
                    # This only terminates idle clients. Already accepted slots drain.
                    stopped.set()
                for future in futures:
                    future.result(timeout=15)
            report['elapsed_ns'] = time.perf_counter_ns() - origin
            report['drained'] = server.recovered()
            report['stopped'] = server.stop()
        accepted = [row for row in records if row['client'] is not None]
        report.update(completed=len(accepted), client_capacity_drops=count - len(accepted),
                      timing=verify.timings(records), status='passed')
        verify.offered(report)
        return report
    except BaseException as error:
        report.update(status='failed', error=f'{type(error).__name__}: {error}')
        raise
    finally:
        # Preserve partial records and errors as well as successful distributions.
        (out / 'offered.json').write_text(json.dumps(report, indent=2) + '\n')
