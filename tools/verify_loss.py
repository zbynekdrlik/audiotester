#!/usr/bin/env python3
"""Independent loss verification for audiotester recordings.

Reads sent_*.bin and recv_*.bin files and replays counter sequence
comparison to detect gaps, independent of the Rust detect_frame_loss().

Usage: python verify_loss.py /path/to/recordings/

Record format: 10 bytes per entry
  - u16_le: counter value (0-65535)
  - u64_le: frame index (monotonic)
"""

import struct
import sys
import glob
import os

RECORD_SIZE = 10  # u16 + u64


def read_records(filepath):
    """Read binary records from file, yield (counter, frame_index) tuples."""
    with open(filepath, "rb") as f:
        while True:
            data = f.read(RECORD_SIZE)
            if len(data) < RECORD_SIZE:
                break
            counter, frame_idx = struct.unpack("<HQ", data)
            yield (counter, frame_idx)


def analyze_gaps(records):
    """Find gaps in counter sequence.

    Returns list of (frame_idx, expected, got, gap_size).
    """
    gaps = []
    prev_counter = None
    for counter, frame_idx in records:
        if prev_counter is not None:
            expected = (prev_counter + 1) & 0xFFFF
            if counter != expected:
                diff = (65536 + counter - expected) & 0xFFFF
                if 0 < diff < 32768:  # Forward gap (not backward/wrap artifact)
                    gaps.append((frame_idx, expected, counter, diff))
        prev_counter = counter
    return gaps


def find_files(directory, prefix):
    """Find and sort recording files by name (chronological order)."""
    pattern = os.path.join(directory, f"{prefix}_*.bin")
    files = sorted(glob.glob(pattern))
    return files


def analyze_direction(directory, prefix, label):
    """Analyze all files for one direction (sent or recv)."""
    files = find_files(directory, prefix)
    if not files:
        print(f"  No {prefix}_*.bin files found")
        return 0, 0

    total_records = 0
    total_gaps = 0
    total_lost = 0

    for filepath in files:
        filename = os.path.basename(filepath)
        records = list(read_records(filepath))
        gaps = analyze_gaps(records)
        lost_in_file = sum(g[3] for g in gaps)

        total_records += len(records)
        total_gaps += len(gaps)
        total_lost += lost_in_file

        if gaps:
            print(f"  {filename}: {len(records)} records, {len(gaps)} gaps, {lost_in_file} lost")
            for frame_idx, expected, got, size in gaps[:10]:  # Show first 10
                print(f"    frame {frame_idx}: expected {expected}, got {got} (gap={size})")
            if len(gaps) > 10:
                print(f"    ... and {len(gaps) - 10} more gaps")
        else:
            print(f"  {filename}: {len(records)} records, no gaps")

    print(f"\n  {label} TOTAL: {total_records} records, {total_gaps} gaps, {total_lost} samples lost")
    return total_gaps, total_lost


def main():
    if len(sys.argv) < 2:
        print("Usage: python verify_loss.py /path/to/recordings/")
        print()
        print("On Windows (iem.lan):")
        print("  python verify_loss.py %APPDATA%\\audiotester\\recordings")
        sys.exit(1)

    directory = sys.argv[1]
    if not os.path.isdir(directory):
        print(f"Error: directory not found: {directory}")
        sys.exit(1)

    print(f"Analyzing recordings in: {directory}")
    print()

    print("=== SENT (output callback) ===")
    sent_gaps, sent_lost = analyze_direction(directory, "sent", "SENT")
    print()

    print("=== RECEIVED (input callback) ===")
    recv_gaps, recv_lost = analyze_direction(directory, "recv", "RECEIVED")
    print()

    print("=== SUMMARY ===")
    print(f"  Sent gaps:     {sent_gaps} ({sent_lost} samples)")
    print(f"  Received gaps: {recv_gaps} ({recv_lost} samples)")

    if sent_gaps == 0 and recv_gaps > 0:
        print()
        print("  CONCLUSION: Losses are REAL (sent clean, received has gaps)")
        print("  The loss detection algorithm is correct.")
    elif sent_gaps > 0 and recv_gaps > 0:
        print()
        print("  WARNING: Both sent and received have gaps!")
        print("  This may indicate a system-level issue (CPU overload, driver problem)")
    elif sent_gaps == 0 and recv_gaps == 0:
        print()
        print("  CLEAN: No losses detected in either direction")
    else:
        print()
        print("  UNEXPECTED: Sent has gaps but received is clean")
        print("  This needs investigation")


if __name__ == "__main__":
    main()
