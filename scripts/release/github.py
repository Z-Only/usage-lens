"""Small GitHub REST client. Tokens stay in the process environment, never artifacts."""
import json
import os
import urllib.error
import urllib.parse
import urllib.request


class GitHubError(RuntimeError):
    def __init__(self, status, message):
        super().__init__(message)
        self.status = status


class GitHub:
    def __init__(self, repository=None, token=None):
        self.repository = repository or os.environ["GITHUB_REPOSITORY"]
        if self.repository != "Z-Only/usage-lens":
            raise ValueError("Release publishing is restricted to Z-Only/usage-lens")
        self.token = token or os.environ["GITHUB_TOKEN"]
        self.base = f"https://api.github.com/repos/{self.repository}"

    def request(self, path, method="GET", value=None, data=None):
        if not path.startswith(("/", "https://")) or path.startswith("//"):
            raise ValueError("Expected a GitHub relative API path or HTTPS upload URL")
        url = path if path.startswith("https://") else self.base + path
        parsed = urllib.parse.urlsplit(url)
        if parsed.scheme != "https" or parsed.hostname not in ("api.github.com", "uploads.github.com"):
            raise ValueError("Unexpected GitHub API host")
        headers = {"Authorization": f"Bearer {self.token}", "Accept": "application/vnd.github+json", "X-GitHub-Api-Version": "2022-11-28", "User-Agent": "usage-lens-release"}
        if value is not None:
            data = json.dumps(value).encode()
            headers["Content-Type"] = "application/json"
        elif data is not None:
            headers["Content-Type"] = "application/octet-stream"
        request = urllib.request.Request(url, data=data, headers=headers, method=method)
        # The endpoints used here do not redirect; never forward auth across a redirect.
        class NoRedirect(urllib.request.HTTPRedirectHandler):
            def redirect_request(self, req, fp, code, msg, hdrs, newurl):
                return None
        try:
            with urllib.request.build_opener(NoRedirect()).open(request, timeout=120) as response:
                return json.load(response)
        except urllib.error.HTTPError as error:
            raise GitHubError(error.code, f"GitHub request failed: HTTP {error.code} {method} {parsed.path}") from None

    def pages(self, path, key=None):
        items = []
        for page in range(1, 101):
            separator = "&" if "?" in path else "?"
            response = self.request(f"{path}{separator}per_page=100&page={page}")
            batch = response[key] if key else response
            items.extend(batch)
            if len(batch) < 100:
                return items
        raise ValueError("API pagination exceeded safe bound")
