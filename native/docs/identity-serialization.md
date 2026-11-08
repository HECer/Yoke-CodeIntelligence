# Immutable identity serialization

`model::identity` hashes the exact bytes emitted by the existing Serde JSON
serializer with SHA-256. Field order, map iteration order, string escaping and
numeric representation retain their released behavior. This is a serialization
contract, not a new canonical JSON format. No analyzer, schema or identity epoch
changes with this implementation.

The serializer now writes into a SHA-256 sink through a 16 KiB buffer instead of
materializing another complete JSON byte vector. Already allocated graph and
source values remain owned by their callers. Custom serializers can allocate
their own memory; this change does not impose a universal allocation or execution
deadline on arbitrary `Serialize` implementations. Serialization failures retain
the existing panic contract and never return a partial identity.

The Windows regression fixture compares the released materialized-byte hash with
the streamed result for more than four megabytes of Unicode and escaped content,
nested values and numeric boundaries. Its observed largest additional allocation
must remain below 64 KiB. The previous implementation requested an 8 MiB buffer
on this fixture. This memory observation is not a project-wide latency claim.
