"""Put OCR blocks back into human reading order.

PP-OCR returns blocks in its own detection order, and on a two-column paper that
weaves the columns together. The sequence metric then reports a page as 65% when
every character was read correctly — the text was merely in the wrong order. For
the product this is not cosmetic: "make this scan searchable" is worthless if the
extracted text reads like a shuffled deck.

No new dependency. The boxes are already in the OCR output, and a two-column page
announces itself: there is a vertical band through the middle that no block
crosses. That band is the gutter.

Known limit: a full-width block *below* the columns (a figure caption, a footer)
is ordered as if it were a heading. Column detection is the 80% win; placing
full-width blocks by their own vertical position is a refinement for later.
"""

BAND = (0.30, 0.70)  # where a gutter may sit, as a fraction of the text width
MIN_SIDE = 0.25  # each column must hold at least this share of the blocks
MAX_CROSS = 0.05  # a genuine gutter is crossed by almost nothing
WIDE = 0.6  # a block this wide spans the page: it cannot inform a gutter


def _x0(block):
    return min(p[0] for p in block["box"])


def _x1(block):
    return max(p[0] for p in block["box"])


def _y0(block):
    return min(p[1] for p in block["box"])


def _by_position(block):
    return (_y0(block), _x0(block))


def find_gutter(blocks):
    """Return the x of the column gutter, or None for a single-column page."""
    if len(blocks) < 8:
        # Too few blocks to tell a gutter from a gap between two words.
        return None

    lo = min(_x0(b) for b in blocks)
    hi = max(_x1(b) for b in blocks)
    width = hi - lo
    if width <= 0:
        return None

    best = None
    for step in range(1, 40):
        frac = BAND[0] + (BAND[1] - BAND[0]) * step / 39
        x = lo + width * frac
        # A full-width block (title, rule, footer) crosses every candidate, so it
        # is evidence about nothing. Counting it rejected the gutter on a page whose
        # only wide block was the title - caught by this module's own self-test.
        crossing = sum(
            1 for b in blocks if (_x1(b) - _x0(b)) < width * WIDE and _x0(b) < x < _x1(b)
        )
        left = sum(1 for b in blocks if _x1(b) <= x)
        right = sum(1 for b in blocks if _x0(b) >= x)
        # Prefer the emptiest band; break ties toward the middle of the page.
        rank = (crossing, abs(frac - 0.5))
        if best is None or rank < best[0]:
            best = (rank, x, crossing, left, right)

    _, x, crossing, left, right = best
    total = len(blocks)
    if crossing <= total * MAX_CROSS and left >= total * MIN_SIDE and right >= total * MIN_SIDE:
        return x
    return None


def order_blocks(blocks):
    """Blocks in human reading order."""
    if not blocks:
        return []

    gutter = find_gutter(blocks)
    if gutter is None:
        return sorted(blocks, key=_by_position)

    straddling = [b for b in blocks if _x0(b) < gutter < _x1(b)]
    left = sorted((b for b in blocks if _x1(b) <= gutter), key=_by_position)
    right = sorted((b for b in blocks if _x0(b) >= gutter), key=_by_position)
    return sorted(straddling, key=_by_position) + left + right


def _selftest():
    def box(x0, y0, x1, y1, text):
        return {"box": [[x0, y0], [x1, y0], [x1, y1], [x0, y1]], "text": text}

    # A two-column page: left (L1..L4) and right (R1..R4), deliberately interleaved
    # in the input, the way a detector that scans left-to-right per line would emit.
    page = [
        box(60, 40, 540, 70, "Title"),
        box(60, 100, 280, 130, "L1"), box(320, 100, 540, 130, "R1"),
        box(60, 140, 280, 170, "L2"), box(320, 140, 540, 170, "R2"),
        box(60, 180, 280, 210, "L3"), box(320, 180, 540, 210, "R3"),
        box(60, 220, 280, 250, "L4"), box(320, 220, 540, 250, "R4"),
    ]
    got = [b["text"] for b in order_blocks(page)]
    want = ["Title", "L1", "L2", "L3", "L4", "R1", "R2", "R3", "R4"]
    assert got == want, f"two-column order wrong:\n got {got}\nwant {want}"

    # A single-column page must be untouched apart from top-to-bottom sorting.
    single = [box(60, y, 540, y + 20, f"P{i}") for i, y in enumerate([200, 40, 120])]
    # y = 200, 40, 120 for P0, P1, P2 -> top-to-bottom is P1, P2, P0.
    assert [b["text"] for b in order_blocks(single)] == ["P1", "P2", "P0"], "single column"

    # Too few blocks: never guess a gutter on two words.
    assert find_gutter(single) is None
    print("  ✅ reading_order 自检通过")


if __name__ == "__main__":
    _selftest()
