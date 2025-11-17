#!/usr/bin/env python3
"""
Gas Analysis Script

This script:
1. Reads dex_spammer log files
2. Extracts transaction hashes from log entries
3. Fetches transaction receipts via RPC
4. Saves all receipts to gas_analyze.json
"""

import argparse
import json
import re
import sys
import requests
from typing import List, Dict, Any


def extract_tx_hashes(log_file_path: str) -> List[str]:
    """
    Parse log file and extract all transaction hashes.

    Expected log format:
    2025-10-13T13:11:14.355726Z  INFO malachitebft_eth_utils::spammer: Sent tx with hash. nonce=540895 tx_hash=0x...
    """
    tx_hashes = []
    tx_hash_pattern = re.compile(r'tx_hash=(0x[a-fA-F0-9]{64})')

    try:
        with open(log_file_path, 'r') as f:
            for line in f:
                match = tx_hash_pattern.search(line)
                if match:
                    tx_hash = match.group(1)
                    tx_hashes.append(tx_hash)

        print(f"Extracted {len(tx_hashes)} transaction hashes from {log_file_path}")
        return tx_hashes

    except FileNotFoundError:
        print(f"Error: File '{log_file_path}' not found", file=sys.stderr)
        sys.exit(1)
    except Exception as e:
        print(f"Error reading file: {e}", file=sys.stderr)
        sys.exit(1)


def fetch_transaction_receipt(tx_hash: str, rpc_url: str) -> Dict[str, Any]:
    """
    Fetch transaction receipt via eth_getTransactionReceipt RPC call.
    """
    payload = {
        "jsonrpc": "2.0",
        "method": "eth_getTransactionReceipt",
        "params": [tx_hash],
        "id": 1
    }

    headers = {
        "Content-Type": "application/json"
    }

    try:
        response = requests.post(rpc_url, json=payload, headers=headers, timeout=10)
        response.raise_for_status()
        result = response.json()

        if "error" in result:
            print(f"RPC error for {tx_hash}: {result['error']}", file=sys.stderr)
            return None

        return result.get("result")

    except requests.exceptions.RequestException as e:
        print(f"Request error for {tx_hash}: {e}", file=sys.stderr)
        return None


def main():
    parser = argparse.ArgumentParser(
        description="Analyze gas usage from dex_spammer logs"
    )
    parser.add_argument(
        "log_file",
        help="Path to the dex_spammer log file"
    )
    parser.add_argument(
        "--rpc-url",
        default="http://127.0.0.1:8545",
        help="RPC endpoint URL (default: http://127.0.0.1:8545)"
    )
    parser.add_argument(
        "--output",
        default="gas_analyze.json",
        help="Output JSON file (default: gas_analyze.json)"
    )

    args = parser.parse_args()

    print(f"Starting gas analysis...")
    print(f"Log file: {args.log_file}")
    print(f"RPC URL: {args.rpc_url}")
    print(f"Output file: {args.output}")
    print()

    # Extract transaction hashes
    tx_hashes = extract_tx_hashes(args.log_file)

    if not tx_hashes:
        print("No transaction hashes found in log file")
        sys.exit(0)

    # Fetch receipts
    receipts = []
    successful = 0
    failed = 0

    print(f"\nFetching transaction receipts...")
    for i, tx_hash in enumerate(tx_hashes, 1):
        print(f"[{i}/{len(tx_hashes)}] Fetching receipt for {tx_hash}...", end=" ")

        receipt = fetch_transaction_receipt(tx_hash, args.rpc_url)

        if receipt:
            receipts.append({
                "tx_hash": tx_hash,
                "receipt": receipt
            })
            successful += 1
            print("✓")
        else:
            failed += 1
            print("✗")

    # Save to JSON file
    print(f"\nSaving results to {args.output}...")
    try:
        with open(args.output, 'w') as f:
            json.dump(receipts, f, indent=2)
        print(f"Successfully saved {len(receipts)} receipts")
    except Exception as e:
        print(f"Error writing output file: {e}", file=sys.stderr)
        sys.exit(1)

    # Calculate gas statistics
    gas_used_values = []
    for item in receipts:
        receipt = item.get("receipt")
        if receipt and receipt.get("gasUsed"):
            # Convert hex string to integer
            gas_used_hex = receipt["gasUsed"]
            if isinstance(gas_used_hex, str) and gas_used_hex.startswith("0x"):
                gas_used = int(gas_used_hex, 16)
            else:
                gas_used = int(gas_used_hex)
            gas_used_values.append(gas_used)

    # Summary
    print("\n=== Summary ===")
    print(f"Total transactions: {len(tx_hashes)}")
    print(f"Successful: {successful}")
    print(f"Failed: {failed}")
    print(f"Output file: {args.output}")

    if gas_used_values:
        avg_gas = sum(gas_used_values) / len(gas_used_values)
        min_gas = min(gas_used_values)
        max_gas = max(gas_used_values)
        total_gas = sum(gas_used_values)

        print("\n=== Gas Usage Statistics ===")
        print(f"Average gas used: {avg_gas:,.2f}")
        print(f"Minimum gas used: {min_gas:,}")
        print(f"Maximum gas used: {max_gas:,}")
        print(f"Total gas used: {total_gas:,}")
    else:
        print("\nNo gas usage data available")


if __name__ == "__main__":
    main()
