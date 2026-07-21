# ARP golden (M0)

Hex → JSON gate for `docs/examples/arp.nfdl` via `parse_and_run_with_data`.

## Packet

`input.hex` is a minimal **Ethernet/IPv4 ARP request** (28 octets), matching the
layout exercised by the existing runtime ARP tests.

| Offset | Len | Field        | Value |
|--------|-----|--------------|-------|
| 0      | 2   | `hw_type`    | 1 (Ethernet) |
| 2      | 2   | `proto_type` | `0x0800` (IPv4) |
| 4      | 1   | `hw_len`     | 6 |
| 5      | 1   | `proto_len`  | 4 |
| 6      | 2   | `opcode`     | 1 (request) |
| 8      | 6   | `sender_mac` | `aa:aa:aa:aa:aa:aa` |
| 14     | 4   | `sender_ip`  | `192.168.1.1` |
| 18     | 6   | `target_mac` | `00:00:00:00:00:00` |
| 24     | 4   | `target_ip`  | `192.168.1.2` |

A reply would use the same layout with `opcode = 2` and a filled `sender_mac`
for the responder; not required for this M0 gate.

## Endian / packing assumptions

- Protocol meta: `endian = big` — multi-byte scalars (`u16` fields) are
  network byte order on the wire.
- Variable `bytes[N]` fields are packed into a `u64` slot by the VM: up to 8
  octets, **big-endian shift-in** (`val = (val << 8) | byte`), remaining length
  beyond 8 is consumed but not stored in the scalar slot. MAC (6) and IPv4 (4)
  therefore appear as integers in `expected.json` (e.g. `0xaaaaaaaaaaaa`,
  `0xc0a80101`).
- `__current_offset` is bytes consumed after a successful datagram parse (28).
- Events today: a single `Message { msg_type: "ArpPacket", size: 28 }` (no EFSM
  on ARP). Field-level JSON is the scalar `ctx` map from the runner.
