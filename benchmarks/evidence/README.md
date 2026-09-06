# Source-keyed slice evidence

These archives preserve raw experiment and qualification output with the source
patches, manifests and binary hashes used to produce it. See the linked review for
commands, ordering, limitations and retention decisions. This is not a complete
release qualification or an archive of executed binaries.

| Bundle | Review | SHA-256 |
| --- | --- | --- |
| `mutex-final-guard-f0ce5246.tar.gz` | [Final guarded checkpoint; zero benchmark invocations](../finishing-pass.md) | `f8a8e01b7b41ade65e1021fe5a1b38b511a3e80672f9eafd6a00a51ba8059a99` |
| `offered-load-e344a60e.tar.gz` | [Bounded fixed offered-load application control](../offered-load-review.md) | `7d6165cae50a9474fc159db946da7e2276f9c0d0bc3f7addb9d5209a9d67186a` |
| `inbox-refill-evidence-9dda2c9f.tar.gz` | [Pre-cleanup refill evidence; historical stall remains open](../inbox-refill-evidence-review.md) | `468a48f2f027b08344a40ba413313a5aa40e6d973e16dacb5c14ed2aebd8c1f4` |
| `deadline-join-order-104957c5.tar.gz` | [Selected deadline survives late child completion](../deadline-join-order-review.md) | `5f341dba055025d7e6f203c2f01de84fc7f393c873a050ade13fdb5d72062f9d` |
| `cross-join-order-d4bf2450.tar.gz` | [Typed cross-runtime interruption and handle recovery](../cross-join-order-review.md) | `65f11cf64108d634b03d15a234dd039a3aa62872d2aba466ce36623184fc83dd` |
| `selected-timer-order-889c2843.tar.gz` | [Selection before delayed timer resumption](../selected-timer-order-review.md) | `0dfba52efb61fd30b8826767307d744da2be42486c3293d25ceee485d8eaf14a` |
| `mutex-mechanism-partial-f0ce5246.tar.gz` | [Partial frozen-source mutex mechanism measurements](../mutex-mechanism-partial-review.md) | `6f36e4a020233e7179eceb285e35f97ed2e792978cf5cddc61ac2e473f402755` |
| `cancellation-history-order-fb834f0f.tar.gz` | [Separate cancellation-history semantic and timing gates](../cancellation-history-order-review.md) | `d809bde75d32fc3d0ee7db1ae9be0e9da5089220c767f21aa0a477c9bb72416e` |
| `io-retry-order-7bb05ae6.tar.gz` | [Ordered readiness retry contract](../io-retry-order-review.md) | `fc1a613a968e627a7e1f2572c59e7c8327a74db749bd5dcdf34edaedc5044c69` |
| `mutex-mechanism-f0ce5246.tar.gz` | [Qualified mutex controls, formal panel pending](../mutex-mechanism-review.md) | `09a0228a1e9f4d6641f49b07f1ccfc78437d16599a39890fdfdeaab4999ca6db` |
| `publication-observer-266e8624.tar.gz` | [Test-only publication observer ordering](../publication-observer-review.md) | `b411fd245b7f5e438da2d2d533570656f700b3ef007a9b39a21efb21f98f220a` |
| `channel-attribution-56f61f86.tar.gz` | [Channel rearming and publication attribution](../channel-attribution-review.md) | `1159959bd818adf21ee07aefa0eb954a16b8b8053326542443984beb8de3e38c` |
| `readiness-incremental-01b33038.tar.gz` | [Bounded incremental readiness, promotion held](../readiness-incremental-review.md) | `aa4241bdf0dba95cae5ac59b5f13dafd882c984c90d0d3bc476bc36d15d7df6c` |
| `capacity-admission-4aaf5b0a.tar.gz` | [Rejected zero/32 admission-only probe experiments](../capacity-admission-review.md) | `8426b17dba434dd5bfafd27a52f2ff7206abefa8408190d6f901ce89f2ddaa09` |
| `publication-progress-90bc3126.tar.gz` | [Native recipient progress repair and measured cost](../publication-progress-review.md) | `10e3d51d55621bfa04f2da5799dc315435609ce5f8b30e9735e1a77b9725f2ae` |
| `publication-deferral-e9214dc3.tar.gz` | [Publication deferral model, not runtime integration](../publication-deferral-review.md) | `bbae5ee124ef57a618aa351d320598d450b9a99d37ab03bc1b54b65eaa7d9144` |
| `native-qualification-f57b1db1.tar.gz` | [Required native qualification and pristine control](../finishing-pass.md) | `966e766560aa17696263f1211f7b0e0ae995792d0475e8d346bf99a487e4e6a8` |
| `ready-fairness-bd80c2f1.tar.gz` | [Ready fairness](../ready-fairness-review.md) | `a60f5ed413af10412e64dbed83a9e8c0dcd5b02fc3735195eaa768f5b58071e8` |
| `wait-qualification-22558d5c.tar.gz` | [Ordered wait qualification](../wait-qualification-review.md) | `eb70809ed123b79cd9d0c3b79cfaef32725b4b2520fc18dfc86d50d42b38518d` |
| `admission-fairness-fc9156a9.tar.gz` | [Admission fairness](../admission-fairness-review.md) | `398d1233d2e195d542768037c64bb4ecb0fb21ec8c7f1c0f84f55cfd720e4850` |
| `completion-observer-f8f1f6f5.tar.gz` | [Completion observer](../completion-observer-review.md) | `b080b4bc24ed6a274cc2755433cb928e052dbfc338c71be341e416ba7f3f4070` |
| `publication-f5cde19e.tar.gz` | [Wake publication](../publication-review.md) | `2fa11ac9c72cee83611daac9c3791231d422acc68216416dad940b0639f5c820` |
| `scheduler-profile-718e21c1.tar.gz` | [Scheduler activity](../scheduler-profile-review.md) | `7dc52997e93f3d3499eb02afa224c35a666fb25f658397c76f6a8a5598b5b397` |
| `capacity-pacing-244bc8a1.tar.gz` | [Rejected capacity experiment](../capacity-pacing-review.md) | `93539843d126e7cc2b1b3c71293c8bf9d64cd1b21a344c1522d80b58240a41b5` |
| `ingress-visibility-aa2fa66e.tar.gz` | [Idle ingress progress](../ingress-visibility-review.md) | `9d3d6c9ec47309922b4cb716c689f00b02ecd46d81f40ccc47faf4e59c5b1bdc` |
| `capacity-ingress-e6474864.tar.gz` | [Four-way capacity attribution](../capacity-pacing-review.md) | `766796d6657116476b550162cfe61046f336bc74f2c2ca7ebe9cf99252ac2836` |
| `shared-channel-control-a373785e.tar.gz` | [Shared channel contract](../shared-channel-review.md) | `1234a27e3570d54498af5a660b29619e6f37bbd84d166eb1f05610c14abe4da9` |
| `channel-eligibility-5048ad21.tar.gz` | [Channel eligibility](../channel-handoff-review.md) | `b15999876d4945c969c1787414985145e47dec463620ef5af273f4a154858e78` |
| `channel-idle-1d8e9448.tar.gz` | [Channel and carrier idling](../channel-handoff-review.md) | `e61c796d888cacff8aa908d46e09ba4294d4fdfdd09c5b1cd255e6641c8da03b` |
| `channel-latency-30bb4869.tar.gz` | [Channel endpoint latency](../channel-latency-review.md) | `631d49058c0d7df9524c26b11edabf8dca6cac33d497d40f079c019217da356a` |
| `channel-tail-c6a2bd0a.tar.gz` | [Channel gains and burst-CPU rejection](../channel-tail-review.md) | `7cd42245ad82c5333b09b0b0c251b03ae2ba4276dc2ea83b67367983f619d861` |
| `adaptive-idle-af71b577.tar.gz` | [Adaptive polling screen](../adaptive-idle-review.md) | `139a5e4cfc729a32ed0ce65e787238dd280f69872d40d786aaa9856357ef9f7b` |
| `handoff-attribution-f57b1db1.tar.gz` | [Useful handoff, polling and capacity attribution](../handoff-attribution-review.md) | `d816a803b2ef6d29556801f7e3ce1978cf63596d9029a3860a6bd1197dad3431` |
| `channel-publication-56f61f86.tar.gz` | [Rejected channel reservation/publication prototypes](../channel-publication-review.md) | `888f1481a6d90424877022aaaf0b1fc77b70ab78e10f637c3e892b6166e043dc` |

The offered-load bundle preserves the bounded application harness, schedule,
busy-client and tail-verifier mutations, missing-matrix control, nineteen evidence
tests, 700 repeated ordered tests, canonical qualification and initial/final live
matrices under their own source identities. Every offered request and all drops
remain visible. This is shared-host harness qualification, not runtime speedup,
controlled-host tail acceptance, HTTP or a new May result.

The inbox-refill-evidence bundle preserves the original failure, ordered
pre-cleanup observations, lost-notification and cleanup-order negative controls,
fourteen-gate qualification, forty targeted repetitions and four oversubscribed
full-library runs. All positive runs pass, but the historical stall's missing
state remains unclassified and release-blocking. No runtime change is retained.

The channel-attribution bundle preserves the missing matching historical A/E
default cycle profiles, a complete 48-process established-counter panel, two
work counts, twelve extended diagnostic processes and counter negative controls.
It also retains the expanded-counter timeout, sampling throttling and final
profile build-overlap exclusion. One extra local rearm reference-count pair per
value is demonstrated in the instrumented fixture, not assigned the entire loss.
No new channel candidate, polling change or May result is promoted.

The readiness-incremental bundle preserves the complete experimental source,
bounded production-state model, old-code and deliberate-mutation negative
controls, 920 repeated native test executions, fourteen-gate qualification and
192 completed counter processes. The mostly idle population workload improves
substantially, but protected CPU, cycle and tail observations do not establish
non-regression. Promotion is held; no readiness runtime change is retained on the
production perf branch. Four guard-stopped groups ran no invocation. All completed
processes remain in the bundle, along with the population control's mechanical
source-extraction caveat. This is not a new May or release-qualification result.

The capacity-admission bundle preserves five default-build arm identities,
negative and passing snapshot/probe/kernel tests, complete source reconstruction,
all 27 counter processes and analyses of the 24 complete guarded controls. The
zero-probe factorial candidate and single 32-probe follow-up both fail the first
tight lifecycle screen. The partial/overlapped 10k group is excluded. No capacity
or polling change is retained, and no full candidate qualification is claimed.

The publication-progress bundle preserves the integrated owner-deferral and
lifetime-safe cleanup repair, actual shared production protocol/MPSC models,
ordered old-code and missing-notification negative controls, fourteen-gate native
qualification, 1,140 repeated ordered test executions and 778,182 mixed lifetimes.
Its cost panel selects 104 complete guarded processes from 181 total invocations;
two observed build overlaps and incomplete/preliminary groups remain separate.
The repair has a repeatable park cycle cost, not a qualified throughput/tail win.
At that checkpoint all six loaded findings remained open. The later ordered I/O
test bundle closes one; see the [current finishing pass](../finishing-pass.md).
The cross-platform release matrix remains open.

The publication-deferral bundle preserves thirteen passing model tests with two
negative controls, an actual proposed abandonment bug, an explicitly classified
SC-adapter false alarm, and a stopped exploration. It includes the limited model's
source patch, final binary hashes and fourteen-gate canonical qualification with
native debug/release. It does not contain a runtime repair or benchmark claim.

The native-qualification bundle preserves a pristine `0b67337` control, the
qualification-only patch, five policy tests with negative controls, and both
canonical attempts. The second passes all 14 gates including default debug and
release workspace execution; the first cancellation-history timing failure stays
open. Thirteen control processes completed, twelve with clean endpoint guards;
one was excluded for an observed host build and seventeen planned runs did not
execute. It is neither a runtime speedup nor a new May panel. Both feature worlds
use the native engine; the earlier compatibility-engine report was incorrect.

The channel-publication bundle preserves five independent candidate patches,
203 completed counter processes (202 in complete analysis pairs), the paused-
publisher old-code failure, targeted native race tests, production-source Loom
adapters and their failed/passing model iterations. All candidates are rejected:
sampled-tail gains do not offset uninstrumented cycle regressions. The final
inlining control does not recover the local penalty. The restored production
checkout's 11-gate canonical receipt, exact source/binary identities, guard
failures, replay scripts and analysis tests are included. No runtime change,
capacity-maintenance change or readiness redesign is retained.

The handoff-attribution bundle contains 72 instrumented/default counter processes,
the diagnostic-only source patch against `88e2fd1`, default `.text` equivalence
hashes, final canonical/native/benchmark qualification and mixed soaks. Four raw
compressed perf recordings retain both useful cycle attribution and two lossy,
perturbative syscall tracing attempts; the latter are not quantitative acceptance
evidence. `LEDGER.md`, `SHA256SUMS` and `analyze.py` provide scope and replay checks.

The ready-fairness bundle includes original, FIFO, cohort-32 and cohort-2 logs;
the original failing production-queue regression; counter CSVs; native tests and
soaks; canonical receipt/logs; environment and source digests; and patches against
`4ce4f7a`. The timed cohort-2 source digest supplies the archive name. Qualification
also refreshed zrail's analysis counts, with no subsequent Rust change.

The wait-qualification bundle includes captured stalled-test and late-service
shutdown failures, diagnostic and passive-observer patches/output, a missing-stop
negative control, ordered-test qualification, and the additional open loaded-suite
findings. Its source patch applies to `3372486`. It retains both the valid canonical
receipt and the earlier report-addition repository-state rejection.

The admission bundle contains real-kernel negative controls, the final check-based
quota patch against `a456324`, canonical/native/benchmark qualification, mixed
soaks, balanced and reversed timing panels, and local/remote park counters. It
preserves the initial default-native failure that prompted the separate ordered-
test slice, as well as the final passing qualification after that repair.

The completion-observer bundle records the naturally failed accounting assertion,
the ordered old-assumption and disabled-flush negative controls, the test-only repair
against `2abae58`, canonical/native/optimized qualification, and 100 one-CPU replays.
It includes the earlier publication-source patch and rejected lock-refresh receipt
to preserve the exact context in which the original assertion failed.

The publication bundle contains the final test-only pause probes against `23822ed`,
native/all-feature/optimized qualification, and 20 one-CPU suite repeats. It confirms
a carrier-wide publication dependency while checking ownership and retirement safety;
it is not a protocol repair or a new performance comparison.

The scheduler-profile bundle preserves the opt-in counter/report source, canonical
and both native feature configurations, benchmark gates, a final-only lifecycle
profile and noisy default controls explicitly excluded from performance acceptance.
Source/binary hashes, the exact macro-grant preview, raw output and analysis/replay
commands are included. No scan-free runtime change is part of that checkpoint.

The capacity-pacing bundle preserves a rejected observer-only patch against
`62378f4`, failing/passing snapshot checks and 30 balanced diagnostic processes.
It exposes the lifecycle batching/polling penalty rather than qualifying a speedup.
The source remains shelved; its full canonical and default-performance gates did
not run. Both instrumented binary identities and raw owner counters are included.

The ingress-visibility bundle contains the separate owner-local progress repair
against `62378f4`, a real-carrier paused-notifier negative control, canonical/native/
optimized qualification, 20 one-CPU replays and 705,249 mixed soak lifetimes. It
preserves initial and final test sources separately, along with mixed default-build
controls, excluded contaminated rounds and endpoint host observations. Its progress
proof passes; quiet-host multi-carrier performance/tail acceptance remains open.

The capacity-ingress bundle retains the rejected combined observer/ingress source
against `a4b5a96`, six targeted checks, 48 four-way diagnostic processes, all source
and binary identities, and the separate uninstrumented cycle sample/annotation.
It demonstrates that removing empty-idle reentry does not recover the lifecycle
batching penalty. No idle policy or scan-free change is retained in production.

The shared-channel-control bundle contains the benchmark-only patch against
`7410285`, exact-delivery negative checks, native capacity/carrier smokes, standalone
benchmark gates and the full canonical receipt. The new shared bounded MPMC control
is vthread-only and does not change or replace the historical May comparisons.

The channel-eligibility bundle preserves two failing native over-notification
regressions, the guarded predicate candidate, its 160-state matrix, 18 passing channel
tests, default controls and final-only owner-counter attribution. Eligibility alone
is rejected because one small multi-carrier population enters much more native waiting.

The channel-idle bundle independently varies a larger bounded parked-task polling
horizon. It retains four source/binary identities, all 80 measured processes and
160 host observations, plus passing default-native workspace tests. The combined
prototype is promising but remains shelved pending channel tails, capacity, idle CPU,
stress and final canonical qualification. It is not a new event-word implementation.

The channel-tail bundle preserves the fixed-policy follow-up: 136 throughput/tail
processes, 24 longer lifecycle controls and 48 public-API idle/burst processes. It
includes exact patches, both source/binary identities, the diagnostic and native
tests, raw results, CPU/host observations and replay/analysis scripts. Fixed long
polling is rejected for roughly 2.6x burst CPU despite channel gains; its full
canonical and soak acceptance did not run. Adaptive pacing is a separate experiment.

The adaptive-idle bundle preserves the next owner-local policy and negative control,
all 65,536 short/long histories, three passing idle tests and 18 channel tests. It
retains 40 screened processes plus a separately excluded/replayed two-run build-
overlap pair, source hashes, raw tails/CPU counters, commands and analysis. The
candidate is shelved for burst-CPU and p99.9 failures despite throughput gains.
Its public-API burst diagnostic source is in the preceding channel-tail bundle.
The restored baseline's separate 11-gate checkpoint receipt/logs are also included;
that canonical result is not qualification of the adaptive runtime candidate.

Extract into a fresh directory and inspect the manifests before replaying commands.
Absolute paths in captured receipts describe the original host, not required output
locations. The ARM64 hosted artifacts are separate and still require durable archival.
