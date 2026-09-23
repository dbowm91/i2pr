# Plan 245 status — M6 Java Streaming stock-response construction-signal attribution corrective

Status: **registered-ready-m6-java-streaming-stock-response-construction-signal-attribution-corrective**.

Plan of record:
plans/implementation/mixed-router-interop/245-m6-java-streaming-stock-response-construction-signal-attribution-corrective.md

## Registration basis

Plan 244 closed as:
passed-m6-java-streaming-reverse-direction-continuous-response-attribution-with-response-packet-not-constructed-boundary

Plan 244 proved Direction A on 3/3 hosted attempts and bound the reverse response epoch correctly, but its construction proxy was derived from Connection's `Resend in` retransmit-timer log.

Exact-pinned Java I2P 2.13.0 source review shows that `Resend in` is conditional: ACK-only sequence-0 non-SYN packets bypass the retransmit-timer block. ConnectionDataReceiver.buildPacket() provides the direct construction log `New OB pkt (acks not yet filled in): ...` and therefore must be observed before concluding that stock Java failed to construct a response packet.

Plan 245 is an observation/attribution corrective. It first distinguishes a Plan-244 proxy false negative from genuine writeData/build suppression, then resumes the retained reverse-direction chain if direct construction is proven.

No Java source patch, helper behavior change, publication change, topology/timing change, or production i2pr correction is authorized.

## Current authority

plan_244 = passed-m6-java-streaming-reverse-direction-continuous-response-attribution-with-response-packet-not-constructed-boundary
plan_245 = registered-ready-m6-java-streaming-stock-response-construction-signal-attribution-corrective

plan_201 = blocked-on-m6-java-streaming-reverse-direction-and-publication-closure-pending-plan245
plan_204 = blocked-on-m6-java-second-family-closure-pending-plan245
plan_205 = retained-deferred-conditional-after-plan218-direct-i2cp-requalification

active_plan = none
next_executable_plan = 245-m6-java-streaming-stock-response-construction-signal-attribution-corrective

milestone6_java_mixed_router_interop = not-yet-passed
milestone6_interoperable = not-yet-claimed
milestone10_final_acceptance = closed