"""Screenshot image format handling — PNG, JPEG, WebP."""

from __future__ import annotations

import blazeweb
import pydantic
import pytest

URL = "https://example.com"

PNG_MAGIC = b"\x89PNG\r\n\x1a\n"
JPEG_MAGIC = b"\xff\xd8\xff"


def _is_webp(data: bytes) -> bool:
    return data[:4] == b"RIFF" and data[8:12] == b"WEBP"


class TestScreenshotFormat:
    def test_default_is_png(self):
        data = blazeweb.screenshot(URL)
        assert data[:8] == PNG_MAGIC

    def test_explicit_png(self):
        data = blazeweb.screenshot(URL, format="png")
        assert data[:8] == PNG_MAGIC

    def test_jpeg_format(self):
        data = blazeweb.screenshot(URL, format="jpeg")
        assert data[:3] == JPEG_MAGIC

    def test_webp_format(self):
        data = blazeweb.screenshot(URL, format="webp")
        assert _is_webp(data)

    def test_jpeg_quality_tradeoff(self):
        hq = blazeweb.screenshot(URL, format="jpeg", quality=95)
        lq = blazeweb.screenshot(URL, format="jpeg", quality=5)
        assert len(lq) < len(hq)

    def test_invalid_format_rejected(self):
        with pytest.raises(pydantic.ValidationError):
            blazeweb.screenshot(URL, format="tiff")

    def test_quality_out_of_range_rejected(self):
        with pytest.raises(pydantic.ValidationError):
            blazeweb.screenshot(URL, format="jpeg", quality=150)


class TestFetchAllFormat:
    def test_default_png(self):
        result = blazeweb.fetch_all(URL)
        assert result.png[:8] == PNG_MAGIC

    def test_jpeg(self):
        result = blazeweb.fetch_all(URL, format="jpeg", quality=80)
        assert result.png[:3] == JPEG_MAGIC
        assert "Example Domain" in result.html

    def test_webp(self):
        result = blazeweb.fetch_all(URL, format="webp")
        assert _is_webp(result.png)
