"""Compare Actor-to-Actor throughput across maiko, actix, and ractor."""

import json
from pathlib import Path
from typing import Optional

from tabulate import tabulate


MESSAGE_COUNTS = (1000, 10000, 100000)
PAYLOAD_SIZES = (32, 256, 1024, 4096)
PAYLOAD_HEADERS = ("32B", "256B", "1KiB", "4KiB")
FRAMEWORKS = (
    ("maiko", "actor_to_actor_transport_maiko"),
    ("actix", "actor_to_actor_transport_actix"),
    ("ractor", "actor_to_actor_transport_ractor"),
)


def load_mps(group: str, messages: int, payload_size: int) -> Optional[int]:
    path = (
        Path("target/criterion")
        / group
        / f"{payload_size}B"
        / str(messages)
        / "new"
        / "estimates.json"
    )
    if not path.exists():
        return None

    with path.open("r", encoding="utf-8") as f:
        mean_ns = float(json.load(f)["mean"]["point_estimate"])
    return round(messages / (mean_ns / 1_000_000_000.0))


def fmt_mps(value: Optional[int]) -> str:
    return "n/a" if value is None else f"{value:,}"


def fmt_ratio(value: Optional[float]) -> str:
    return "n/a" if value is None else f"{value:.2f}x"


def build_absolute_table(data: dict[str, dict[int, dict[int, Optional[int]]]]) -> str:
    rows: list[list[str]] = []
    for framework, _group in FRAMEWORKS:
        for messages in MESSAGE_COUNTS:
            row = [framework, f"{messages:,}"]
            for payload_size in PAYLOAD_SIZES:
                row.append(fmt_mps(data[framework][messages][payload_size]))
            rows.append(row)

    return tabulate(
        rows,
        headers=["Framework", "N \\ Payload", *PAYLOAD_HEADERS],
        tablefmt="rounded_outline",
        numalign="right",
        stralign="right",
    )


def build_relative_table(data: dict[str, dict[int, dict[int, Optional[int]]]]) -> str:
    rows: list[list[str]] = []
    for framework in ("actix", "ractor"):
        for messages in MESSAGE_COUNTS:
            row = [framework, f"{messages:,}"]
            for payload_size in PAYLOAD_SIZES:
                base = data["maiko"][messages][payload_size]
                comp = data[framework][messages][payload_size]
                ratio = None if base in (None, 0) or comp is None else comp / base
                row.append(fmt_ratio(ratio))
            rows.append(row)

    return tabulate(
        rows,
        headers=["Framework", "N \\ Payload", *PAYLOAD_HEADERS],
        tablefmt="rounded_outline",
        numalign="right",
        stralign="right",
    )


def main() -> None:
    data: dict[str, dict[int, dict[int, Optional[int]]]] = {}
    for framework, group in FRAMEWORKS:
        data[framework] = {}
        for messages in MESSAGE_COUNTS:
            data[framework][messages] = {}
            for payload_size in PAYLOAD_SIZES:
                data[framework][messages][payload_size] = load_mps(
                    group, messages, payload_size
                )

    print("Absolute throughput")
    print(build_absolute_table(data))
    print("\nUnit: messages/sec")

    print("\nRelative throughput vs maiko")
    print(build_relative_table(data))
    print("\nUnit: multiplier (higher is better)")


if __name__ == "__main__":
    main()
