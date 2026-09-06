"""Verify every offered arrival, bounded client occupancy and endpoint evidence."""

import hashlib
import struct

from application_verify import stopped


def bounds(rates, count):
    assert len(rates) <= 4 and len(set(rates)) == len(rates)
    assert isinstance(count, int) and 1 <= count <= 100_000
    assert all(isinstance(rate, int) and 1 <= rate <= 1_000_000 for rate in rates)
    assert all(count <= rate * 5 for rate in rates), 'offered duration exceeds five seconds'


def quantiles(values):
    assert all(isinstance(value, int) and value >= 0 for value in values)
    if not values:
        return None
    ordered = sorted(values)
    result = {name: ordered[(len(values) * numerator + denominator - 1) // denominator - 1]
              for name, numerator, denominator in [('p50', 50, 100), ('p90', 90, 100),
                  ('p99', 99, 100), ('p999', 999, 1000), ('p9999', 9999, 10000)]}
    return dict(result, maximum=ordered[-1])


def timings(records):
    accepted = [row for row in records if row['client'] is not None]
    return dict(
        dispatch_lag_ns=quantiles([r['offered_ns'] - r['scheduled_ns'] for r in records]),
        arrival_to_admission_ns=quantiles([r['admitted_ns'] - r['scheduled_ns'] for r in accepted]),
        arrival_to_start_ns=quantiles([r['started_ns'] - r['scheduled_ns'] for r in accepted]),
        arrival_to_completion_ns=quantiles([r['completed_ns'] - r['scheduled_ns'] for r in accepted]),
        offered_to_completion_ns=quantiles([r['completed_ns'] - r['offered_ns'] for r in accepted]),
        exchange_ns=quantiles([r['exchange_ns'] for r in accepted]))


def offered(report):
    assert report['schema'] == 1 and report['kind'] == 'offered-load' and report['status'] == 'passed'
    assert report['model'] == 'fixed-offered-drop-if-busy'
    assert report['input_bytes'] == 64 and report['output_bytes'] == 256
    count, rate, concurrency = report['count'], report['rate'], report['concurrency']
    bounds([rate], count)
    assert 1 <= concurrency <= 256 and 1 <= report['carriers'] <= 16
    records, clients = report['records'], report['clients']
    assert len(records) == count and len(clients) == concurrency
    assert [r['sequence'] for r in records] == list(range(count)), 'missing or reordered arrival'
    assert [c['client'] for c in clients] == list(range(concurrency))
    by_client = [[] for _ in clients]
    dropped, last_offer = 0, 0
    for sequence, record in enumerate(records):
        assert record['scheduled_ns'] == sequence * 1_000_000_000 // rate, 'completion-paced schedule'
        assert record['offered_ns'] >= max(last_offer, record['scheduled_ns'])
        last_offer = record['offered_ns']
        index = record['client']
        if index is None:
            dropped += 1
            assert set(record) == {'sequence', 'scheduled_ns', 'offered_ns', 'client'}
        else:
            assert isinstance(index, int) and 0 <= index < concurrency
            assert 0 <= record['offered_ns'] <= record['admitted_ns'] <= record['started_ns'] < record['completed_ns']
            assert 0 < record['exchange_ns'] <= record['completed_ns'] - record['started_ns']
            by_client[index].append(record)
    assert report['completed'] == count - dropped > 0
    assert report['client_capacity_drops'] == dropped
    for client, rows in zip(clients, by_client):
        assert 'error' not in client and client['admission_ns'] > 0
        assert client['sequences'] == [r['sequence'] for r in rows]
        digest, previous_completion = hashlib.sha256(), 0
        for row in rows:
            assert row['admitted_ns'] >= previous_completion, 'client slot reused before completion'
            previous_completion = row['completed_ns']
            sequence = row['sequence']
            payload = [(sequence + offset * 17) % 256 for offset in range(64)]
            digest.update(struct.pack('!Q', sequence))
            digest.update(bytes(payload[offset % 64] ^ ((sequence + offset) % 256) for offset in range(256)))
        assert client['response_sha256'] == digest.hexdigest(), 'payload digest mismatch'
    assert report['timing'] == timings(records)
    assert report['elapsed_ns'] >= report['offer_elapsed_ns'] >= last_offer
    assert report['parked']['active'] == report['parked']['io_readers'] == concurrency
    assert report['parked_resource']['rss_kib'] is not None
    drained = report['drained']
    assert drained['requests'] == report['completed']
    assert drained['accepted'] == drained['closed'] == concurrency
    assert all(drained[key] == 0 for key in ('active', 'pending', 'rejected', 'deadlines', 'malformed', 'disconnected'))
    stopped(report['stopped'])
