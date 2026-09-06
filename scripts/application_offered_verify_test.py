"""Negative controls reject lost arrivals, early slot reuse and censored tails."""

import copy
import hashlib
import json
from pathlib import Path
import unittest

from application_client import expected
import application_offered_verify as verify
import application_verify


def fixture():
    template = json.loads((Path(__file__).parent / 'fixtures/application/load.json').read_text())
    records = [dict(sequence=i, scheduled_ns=i * 1000000, offered_ns=i * 1000000,
                    client=None) for i in range(4)]
    for i, end in [(0, 1500000), (2, 2900000)]:
        records[i].update(client=0, admitted_ns=records[i]['offered_ns'] + 1,
                          started_ns=records[i]['offered_ns'] + 10,
                          completed_ns=end, exchange_ns=100)
    return dict(schema=1, kind='offered-load', status='passed', carriers=1, concurrency=1,
                rate=1000, count=4, model='fixed-offered-drop-if-busy', input_bytes=64,
                output_bytes=256, clients=[dict(client=0, sequences=[0, 2], admission_ns=1,
                    response_sha256=hashlib.sha256(expected(0) + expected(2)).hexdigest())],
                records=records, completed=2, client_capacity_drops=2,
                timing=verify.timings(records), offer_elapsed_ns=3000001, elapsed_ns=4000000,
                parked=dict(active=1, io_readers=1), parked_resource=dict(rss_kib=100),
                drained=dict(requests=2, accepted=1, closed=1, active=0, pending=0,
                             rejected=0, deadlines=0, malformed=0, disconnected=0),
                stopped=template['stopped'])


class OfferedVerificationTests(unittest.TestCase):
    def test_valid_partial_admission_keeps_all_offers(self):
        verify.offered(fixture())

    def test_missing_samples_invalid_payload_and_censored_tails_fail(self):
        mutations = [
            lambda r: r['records'].pop(),
            lambda r: r['records'][1].update(sequence=0),
            lambda r: r['records'][1].update(scheduled_ns=1500000, offered_ns=1500000),
            lambda r: r['records'][2].update(admitted_ns=0),
            lambda r: r['records'][0].pop('completed_ns'),
            lambda r: r['records'][1].update(completed_ns=1000001),
            lambda r: r.update(client_capacity_drops=0),
            lambda r: r.update(completed=0),
            lambda r: r['clients'][0].update(response_sha256='0' * 64),
            lambda r: r['clients'][0].update(sequences=[0, 1]),
            lambda r: r['timing']['arrival_to_completion_ns'].update(p9999=1),
            lambda r: r['timing']['dispatch_lag_ns'].update(maximum=1),
            lambda r: r['drained'].update(requests=4),
            lambda r: r['drained'].update(rejected=2),
            lambda r: r.update(model='closed-loop'),
        ]
        for index, mutate in enumerate(mutations):
            damaged = copy.deepcopy(fixture())
            mutate(damaged)
            with self.subTest(index=index), self.assertRaises((AssertionError, KeyError)):
                verify.offered(damaged)

    def test_slot_reuse_must_follow_previous_completion_not_just_start(self):
        damaged = fixture()
        damaged['records'][0]['completed_ns'] = 2100000
        damaged['timing'] = verify.timings(damaged['records'])
        with self.assertRaisesRegex(AssertionError, 'slot reused'):
            verify.offered(damaged)

    def test_offer_can_precede_completion_if_slot_claim_follows_it(self):
        valid = fixture()
        valid['records'][0]['completed_ns'] = 2000001
        valid['records'][2].update(admitted_ns=2000002, started_ns=2000003)
        valid['timing'] = verify.timings(valid['records'])
        verify.offered(valid)

    def test_nearest_rank_extrema_and_empty_subset(self):
        values = list(range(10000))
        self.assertEqual(verify.quantiles(values), dict(p50=4999, p90=8999, p99=9899,
                                                     p999=9989, p9999=9998, maximum=9999))
        self.assertIsNone(verify.quantiles([]))
        for invalid in [[-1], [1.1]]:
            with self.assertRaises(AssertionError):
                verify.quantiles(invalid)

    def test_schedule_storage_duration_and_matrix_are_bounded(self):
        for rates, count in [([0], 1), ([1000001], 1), ([1, 1], 1), ([1], 6),
                             ([1000000], 100001), ([1, 2, 3, 4, 5], 1)]:
            with self.assertRaises(AssertionError):
                verify.bounds(rates, count)
        args = dict(carriers=[1, 2, 4, 8], concurrency=list(range(1, 9)), rounds=1,
                    fault_rounds=1, offered_rates=[20000, 30000, 40000, 50000], offered_count=100000)
        with self.assertRaises(AssertionError):
            application_verify.matrix(args)


if __name__ == '__main__':
    unittest.main()
