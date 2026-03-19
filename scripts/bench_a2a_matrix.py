"""Print Actor-to-Actor throughput matrix from Criterion estimates."""

import json
from pathlib import Path

from tabulate import tabulate


def main() -> None:
    # Message counts and payload sizes must match the benchmark definition:
    # maiko/benches/actor_to_actor_transport.rs
    message_counts = (1000, 10000, 100000)
    payload_sizes = (32, 256, 1024, 4096)
    headers = ["N \\ Payload", "32B", "256B", "1KiB", "4KiB"]

    rows = []
    for n in message_counts:
        row = [f"{n:,}"]
        for p in payload_sizes:
            path = Path(
                "target/criterion/actor_to_actor_transport"
            ) / f"{p}B" / str(n) / "new" / "estimates.json"
            if not path.exists():
                row.append("n/a")
                continue

            with path.open("r", encoding="utf-8") as f:
                mean_ns = float(json.load(f)["mean"]["point_estimate"])

            mps = round(n / (mean_ns / 1_000_000_000.0))
            row.append(f"{mps:,}")
        rows.append(row)

    print(
        tabulate(
            rows,
            headers=headers,
            tablefmt="rounded_outline",
            numalign="right",
            stralign="right",
        )
    )
    print("\nUnit: messages/sec")


if __name__ == "__main__":
    main()
