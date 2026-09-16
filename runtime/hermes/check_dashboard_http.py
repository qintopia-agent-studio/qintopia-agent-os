#!/usr/bin/env python3
"""Run the real dashboard against a disposable home, never a production profile."""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import re
import socket
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--core-dir", required=True, type=Path)
    parser.add_argument("--dist", required=True, type=Path)
    args = parser.parse_args()
    core, dist = args.core_dir.resolve(strict=True), args.dist.resolve(strict=True)
    with tempfile.TemporaryDirectory(prefix="hermes-dashboard-http-") as temporary:
        home = Path(temporary).resolve()
        with socket.socket() as reserve:
            reserve.bind(("127.0.0.1", 0))
            port = reserve.getsockname()[1]
        # The child must not inherit production credentials, proxy settings, or HOME.
        env = {"HOME": str(home), "HERMES_HOME": str(home), "HERMES_WEB_DIST": str(dist),
               "PATH": os.path.dirname(sys.executable) + ":/usr/bin:/bin", "PYTHONDONTWRITEBYTECODE": "1"}
        bootstrap = (
            "import socket,sys,runpy; "
            "sys.path.insert(0,sys.argv.pop(1)); "
            "original=socket.socket.connect; "
            "socket.socket.connect=lambda self,address: original(self,address) if isinstance(address,tuple) and address[0] in ('127.0.0.1','::1') else (_ for _ in ()).throw(RuntimeError('external network forbidden')); "
            "sys.argv[0]='hermes'; runpy.run_module('hermes_cli.main',run_name='__main__')"
        )
        with (home / "process.log").open("wb") as log:
            process = subprocess.Popen([sys.executable, "-B", "-c", bootstrap, str(core), "dashboard", "--host", "127.0.0.1", "--port", str(port), "--no-open", "--skip-build"], env=env, cwd=core, stdout=log, stderr=log)
            try:
                opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
                base = f"http://127.0.0.1:{port}"
                for _ in range(100):
                    if process.poll() is not None:
                        raise RuntimeError("isolated_dashboard_exited: " + (home / "process.log").read_text()[-2000:])
                    try:
                        with opener.open(base, timeout=1) as response:
                            html = response.read().decode()
                        break
                    except OSError:
                        time.sleep(0.2)
                else:
                    raise RuntimeError("isolated_dashboard_start_timeout")
                token = re.search(r'window\.__HERMES_SESSION_TOKEN__\s*=\s*"([^"]+)"', html)
                assert token, "session token absent"
                for asset in re.findall(r'(?:src|href)="(/assets/[^"?#]+)', html):
                    with opener.open(base + asset, timeout=5) as response:
                        assert response.status == 200 and "text/html" not in response.headers.get("Content-Type", "")
                for route in ["/api/profiles", "/api/sessions?limit=1", "/api/cron/jobs"]:
                    request = urllib.request.Request(base + route, headers={"X-Hermes-Session-Token": token[1]})
                    with opener.open(request, timeout=5) as response:
                        assert response.status == 200
                        json.loads(response.read())
                try:
                    opener.open(base + "/api/profiles", timeout=5)
                except urllib.error.HTTPError as error:
                    assert error.code in {401, 403}
                else:
                    raise AssertionError("API accepted missing session token")
                print("hermes_dashboard_http=passed profiles=synthetic external_network=blocked")
            finally:
                process.terminate()
                try:
                    process.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait()


if __name__ == "__main__":
    main()
