# Contracts

The WIT packages of florn. The WIT text is the contract of record (ARCH-004); each file here is
byte-identical to the block in the RFC that defines it, and a test in `florn-abi` keeps it so.

| Package | File | Defined in |
|---|---|---|
| `florn:core@0.1.0` | `florn-core/wit/core.wit` | RFC 002 §4 |
| `florn:network@0.1.0` | `florn-network/wit/network.wit` | RFC 008 §6 |

A contract whose RFC gives it in several blocks (the circuit's in RFC 012) is the blocks that belong to it, in the order they appear, one blank line between two of them; the same test reads them.
