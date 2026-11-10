#!/usr/bin/env python3
"""Run the SQL security suite in disposable Postgres, without opening ports.

Docker must be running. This tests SQL/RLS, not Supabase Auth or Storage APIs.
"""

from pathlib import Path
import subprocess
import time
import uuid


def run_suite():
    root = Path(__file__).resolve().parents[2]
    name = f"folio-security-{uuid.uuid4().hex[:12]}"
    try:
        subprocess.run(
            [
                "docker", "run", "--rm", "-d", "--network", "none",
                "--name", name, "-e", "POSTGRES_HOST_AUTH_METHOD=trust",
                "postgres:17-alpine",
            ],
            check=True, capture_output=True, text=True,
        )
        deadline = time.monotonic() + 60
        while time.monotonic() < deadline:
            ready = subprocess.run(
                ["docker", "exec", name, "pg_isready", "-h", "127.0.0.1", "-U", "postgres"],
                capture_output=True,
            )
            # The initial bootstrap server only listens on a Unix socket. Wait
            # for TCP to avoid racing its shutdown and the final server startup.
            if ready.returncode == 0:
                break
            time.sleep(0.2)
        else:
            raise RuntimeError("Postgres did not become ready within 60 seconds")
        migrations = sorted((root / "supabase/migrations").glob("*.sql"))
        for path in [root / "supabase/tests/postgres-bootstrap.sql", *migrations,
                     root / "supabase/tests/sync-security.sql"]:
            relative = path.relative_to(root)
            result = subprocess.run(
                ["docker", "exec", "-i", name, "psql", "-h", "127.0.0.1", "-U", "postgres", "-v", "ON_ERROR_STOP=1", "-q"],
                input=path.read_text(), capture_output=True, text=True,
            )
            if result.returncode:
                raise RuntimeError(f"{relative} failed:\n{result.stderr}")
            print(f"PASS {relative}", flush=True)
    finally:
        subprocess.run(["docker", "rm", "-f", name], capture_output=True)


if __name__ == "__main__":
    run_suite()
