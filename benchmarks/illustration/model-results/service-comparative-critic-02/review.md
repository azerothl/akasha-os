# Critic protocol failure — not artistic acceptance

Five real service passes published at 7.8, 14.5, 21.6, 28.3 and 35.6 seconds.
The critic saw ONE image but returned regressed=true, violating its comparison
contract. The old benchmark discarded the correction and exported the baseline
with a technical PASS. That PASS does not validate quality or candidate rejection.
No retouch was executed in this run.

The current benchmark rejects a regression claim at index zero with an error
and no export. This prevents contradictory critic output from appearing to be
a successful comparative evaluation. The actual critic response and baseline
export are kept as evidence of the failure, not approved assets.
