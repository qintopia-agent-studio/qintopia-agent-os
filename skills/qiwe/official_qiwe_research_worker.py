from __future__ import annotations

import argparse
import asyncio
import json
import re
import ssl
import sys
import urllib.request
from dataclasses import dataclass
from html.parser import HTMLParser
from typing import Any, Callable, Sequence
from urllib.parse import urljoin, urlsplit, urlunsplit


WORKER_SCHEMA_VERSION = 1
WORKER_CONTENT_TRUST = "untrusted_reference_data"
QIWE_OFFICIAL_ENTRY_PAGES = (
    "https://doc.qiweapi.com/doc-7331304",
    "https://doc.qiweapi.com/doc-9079960",
)
MAX_RESEARCH_PAGE_BYTES = 128 * 1024
MAX_RESEARCH_TEXT_CHARS = 20_000
MAX_RESEARCH_PAGES = 4
MAX_RESEARCH_DEPTH = 2
MAX_RESEARCH_REQUESTS = 12
MAX_RESEARCH_LINKS_PER_PAGE = 128
MAX_RESEARCH_URL_CHARS = 2_048
MAX_RESEARCH_WORKER_OUTPUT_BYTES = 384 * 1024
RESEARCH_REQUEST_TIMEOUT_SECONDS = 4


class _VisibleTextParser(HTMLParser):
    def __init__(self) -> None:
        super().__init__(convert_charrefs=True)
        self._ignored_depth = 0
        self.parts: list[str] = []
        self.links: list[str] = []

    def handle_starttag(self, tag: str, attrs: list[tuple[str, str | None]]) -> None:
        normalized_tag = tag.lower()
        if normalized_tag in {"script", "style", "noscript", "svg"}:
            self._ignored_depth += 1
        if normalized_tag == "a" and not self._ignored_depth:
            href = next(
                (value for name, value in attrs if name.lower() == "href"), None
            )
            if href and len(self.links) < MAX_RESEARCH_LINKS_PER_PAGE:
                self.links.append(href)

    def handle_endtag(self, tag: str) -> None:
        if (
            tag.lower() in {"script", "style", "noscript", "svg"}
            and self._ignored_depth
        ):
            self._ignored_depth -= 1

    def handle_data(self, data: str) -> None:
        if not self._ignored_depth:
            text = re.sub(r"\s+", " ", data).strip()
            if text:
                self.parts.append(text)


@dataclass(frozen=True)
class WorkerResearchPage:
    url: str
    text: str
    links: tuple[str, ...] = ()


class _NoRedirectHandler(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, *_args: Any, **_kwargs: Any) -> None:
        return None


class _StdlibResponseContent:
    def __init__(self, response: Any) -> None:
        self._response = response

    async def read(self, limit: int) -> bytes:
        return self._response.read(limit)


class _StdlibResponseContext:
    def __init__(
        self,
        opener: Any,
        url: str,
        headers: dict[str, str],
        timeout: int,
    ) -> None:
        self._opener = opener
        self._request = urllib.request.Request(url, headers=headers, method="GET")
        self._timeout = timeout
        self._response: Any | None = None

    async def __aenter__(self) -> _StdlibResponseContext:
        try:
            self._response = self._opener.open(
                self._request,
                timeout=self._timeout,
            )
        except Exception as exc:
            close = getattr(exc, "close", None)
            if callable(close):
                close()
            raise
        self.status = self._response.getcode()
        self.url = self._response.geturl()
        self.headers = self._response.headers
        self.content = _StdlibResponseContent(self._response)
        return self

    async def __aexit__(self, *_args: Any) -> bool:
        if self._response is not None:
            self._response.close()
        return False


class _StdlibClientSession:
    def __init__(self, *, trust_env: bool) -> None:
        if trust_env:
            raise ValueError("official research must not trust the process environment")
        self._opener = urllib.request.build_opener(
            urllib.request.ProxyHandler({}),
            _NoRedirectHandler(),
            urllib.request.HTTPSHandler(context=ssl.create_default_context()),
        )

    async def __aenter__(self) -> _StdlibClientSession:
        return self

    async def __aexit__(self, *_args: Any) -> bool:
        return False

    def get(
        self,
        url: str,
        *,
        allow_redirects: bool,
        headers: dict[str, str],
        proxy: None,
        timeout: int,
    ) -> _StdlibResponseContext:
        if allow_redirects or proxy is not None:
            raise ValueError("official research network policy is invalid")
        return _StdlibResponseContext(self._opener, url, headers, timeout)


def normalize_official_qiwe_url(
    value: str, *, base_url: str | None = None
) -> str | None:
    if (
        not isinstance(value, str)
        or not value
        or len(value) > MAX_RESEARCH_URL_CHARS
        or any(ord(character) <= 0x20 or ord(character) == 0x7F for character in value)
    ):
        return None
    try:
        resolved = urljoin(base_url or "", value)
        parsed = urlsplit(resolved)
        port = parsed.port
    except (TypeError, ValueError):
        return None
    host = (parsed.hostname or "").lower().rstrip(".")
    if (
        parsed.scheme != "https"
        or parsed.username is not None
        or parsed.password is not None
        or port is not None
        or host != "doc.qiweapi.com"
        or parsed.query
        or re.fullmatch(r"/doc-[0-9]+", parsed.path) is None
    ):
        return None
    return urlunsplit(("https", host, parsed.path, "", ""))


async def research_official_qiwe_documents(
    *,
    max_depth: int = MAX_RESEARCH_DEPTH,
    max_pages: int = MAX_RESEARCH_PAGES,
    client_session_factory: Callable[..., Any] | None = None,
) -> list[WorkerResearchPage]:
    bounded_depth = min(max(int(max_depth), 0), MAX_RESEARCH_DEPTH)
    bounded_pages = min(max(int(max_pages), 1), MAX_RESEARCH_PAGES)
    if any(
        normalize_official_qiwe_url(url) != url for url in QIWE_OFFICIAL_ENTRY_PAGES
    ):
        return []

    if client_session_factory is None:
        client_session_factory = _StdlibClientSession

    pages: list[WorkerResearchPage] = []
    queue = [(url, 0) for url in QIWE_OFFICIAL_ENTRY_PAGES]
    visited: set[str] = set()
    request_count = 0
    try:
        async with client_session_factory(trust_env=False) as session:
            while (
                queue
                and len(pages) < bounded_pages
                and request_count < MAX_RESEARCH_REQUESTS
            ):
                candidate, depth = queue.pop(0)
                url = normalize_official_qiwe_url(candidate)
                if url is None or url in visited:
                    continue
                if depth == 0 and url not in QIWE_OFFICIAL_ENTRY_PAGES:
                    return []
                visited.add(url)
                request_count += 1
                page = await _fetch_one(session, url)
                if page is None:
                    continue
                pages.append(page)
                if depth < bounded_depth:
                    for href in page.links:
                        child = normalize_official_qiwe_url(href, base_url=url)
                        if child is not None and child not in visited:
                            queue.append((child, depth + 1))
    except Exception:
        return []
    return pages


async def _fetch_one(session: Any, url: str) -> WorkerResearchPage | None:
    normalized_url = normalize_official_qiwe_url(url)
    if normalized_url is None:
        return None
    try:
        async with session.get(
            normalized_url,
            allow_redirects=False,
            headers={
                "Accept": "text/html,application/json;q=0.8,text/plain;q=0.7",
                "Accept-Encoding": "identity",
                "User-Agent": "qintopia-official-qiwe-research/1",
            },
            proxy=None,
            timeout=RESEARCH_REQUEST_TIMEOUT_SECONDS,
        ) as response:
            if (
                response.status != 200
                or normalize_official_qiwe_url(str(response.url)) != normalized_url
            ):
                return None
            content_type = response.headers.get("Content-Type", "").split(";", 1)[0]
            content_type = content_type.strip().lower()
            if content_type not in {
                "text/html",
                "text/plain",
                "application/json",
            }:
                return None
            content_encoding = response.headers.get("Content-Encoding", "")
            if content_encoding.strip().lower() not in {"", "identity"}:
                return None
            body = await response.content.read(MAX_RESEARCH_PAGE_BYTES + 1)
            if not isinstance(body, bytes) or len(body) > MAX_RESEARCH_PAGE_BYTES:
                return None
    except Exception:
        return None

    text = body.decode("utf-8", errors="replace").replace("\x00", "\ufffd")
    if content_type == "text/html":
        parser = _VisibleTextParser()
        try:
            parser.feed(text)
            parser.close()
        except Exception:
            return None
        text = "\n".join(parser.parts)
        links = tuple(parser.links)
    else:
        links = ()
    text = text[:MAX_RESEARCH_TEXT_CHARS].strip()
    if not text:
        return None
    return WorkerResearchPage(
        url=normalized_url,
        text=text,
        links=links,
    )


def encode_worker_result(pages: Sequence[WorkerResearchPage]) -> bytes:
    if len(pages) > MAX_RESEARCH_PAGES:
        raise ValueError("research worker returned too many pages")
    output_pages: list[dict[str, str]] = []
    seen: set[str] = set()
    for page in pages:
        url = normalize_official_qiwe_url(page.url)
        if (
            url is None
            or url != page.url
            or url in seen
            or not isinstance(page.text, str)
            or not page.text
            or "\x00" in page.text
            or len(page.text) > MAX_RESEARCH_TEXT_CHARS
        ):
            raise ValueError("research worker page is invalid")
        seen.add(url)
        output_pages.append({"url": url, "text": page.text})
    payload = json.dumps(
        {
            "schema_version": WORKER_SCHEMA_VERSION,
            "content_trust": WORKER_CONTENT_TRUST,
            "pages": output_pages,
        },
        ensure_ascii=False,
        separators=(",", ":"),
    ).encode("utf-8")
    if len(payload) > MAX_RESEARCH_WORKER_OUTPUT_BYTES:
        raise ValueError("research worker output exceeds limit")
    return payload


def parse_worker_args(argv: Sequence[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(allow_abbrev=False)
    parser.add_argument("--max-depth", type=int, default=MAX_RESEARCH_DEPTH)
    parser.add_argument("--max-pages", type=int, default=MAX_RESEARCH_PAGES)
    args = parser.parse_args(argv)
    if not 0 <= args.max_depth <= MAX_RESEARCH_DEPTH:
        parser.error("max depth is outside the fixed worker limit")
    if not 1 <= args.max_pages <= MAX_RESEARCH_PAGES:
        parser.error("max pages is outside the fixed worker limit")
    return args


def main(argv: Sequence[str] | None = None) -> int:
    try:
        args = parse_worker_args(argv)
        pages = asyncio.run(
            research_official_qiwe_documents(
                max_depth=args.max_depth,
                max_pages=args.max_pages,
            )
        )
        output = encode_worker_result(pages)
        written = sys.stdout.buffer.write(output)
        sys.stdout.buffer.flush()
        return 0 if written == len(output) else 1
    except Exception:
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
