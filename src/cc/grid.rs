//! The control centre's grid, as layout alone: widgets as rectangles of cells (x, y, w, h) on a grid cols wide,
//! none overlapping. A widget moved or resized takes its place and pushes the ones in its way down; then all
//! float up as far as they go, so the grid never has holes a widget could fill (iOS's widgets, gridstack's way).

/// The width widgets' sizes are declared for: a size of its 8 a share of whatever width a panel has (fit).
pub const BASE: u8 = 8;

/// A widget's sizes, declared for BASE, on a grid cols wide: the whole width the whole width, the others their
/// share of it (half half), at least a cell; each once, in their order.
pub fn fit(sizes: &[(u8, u8)], cols: u8) -> Vec<(u8, u8)> {
    let mut out: Vec<(u8, u8)> = Vec::new();
    for &(w, h) in sizes {
        let w = if w >= BASE { cols } else { ((w as u32 * cols as u32 + BASE as u32 / 2) / BASE as u32).clamp(1, cols as u32) as u8 };
        if !out.contains(&(w, h)) {
            out.push((w, h));
        }
    }
    out
}

/// A width of cells on a grid cols wide as cells of BASE: its share of the width, at least 1.
pub fn base(w: u8, cols: u8) -> u8 {
    if w >= cols { BASE } else { ((w as u32 * BASE as u32 + cols as u32 / 2) / cols as u32).max(1) as u8 }
}

/// A layout laid out for one width on another: each item's left and right edges moved in proportion, so items side
/// by side stay side by side (two halves two halves, on an odd width too); then settled.
pub fn rescale(items: &mut Vec<Item>, from: u8, to: u8) {
    if from == to || from == 0 {
        return;
    }
    let edge = |v: u8| ((v as u32 * to as u32 + from as u32 / 2) / from as u32) as u8;
    for it in items.iter_mut() {
        let (x, end) = (edge(it.x), edge((it.x + it.w).min(from)));
        it.x = x.min(to - 1);
        it.w = end.saturating_sub(x).max(1);
    }
    settle(items, to);
}

#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    /// the widget instance's key, unique on the grid
    pub key: String,
    pub x: u8,
    pub y: u8,
    pub w: u8,
    pub h: u8,
}

impl Item {
    fn overlaps(&self, o: &Item) -> bool {
        self.x < o.x + o.w && o.x < self.x + self.w && self.y < o.y + o.h && o.y < self.y + self.h
    }
}

/// Every item within the grid's width, and wide no more than it.
fn clamp(items: &mut [Item], cols: u8) {
    for it in items.iter_mut() {
        it.w = it.w.clamp(1, cols);
        it.h = it.h.max(1);
        it.x = it.x.min(cols - it.w);
    }
}

/// Items laid out without overlaps, as a hand-edited file may not have them: each in order (higher, then lefter)
/// kept where it is unless it lies on one before it, then put in the first place it fits; then compacted.
pub fn settle(items: &mut Vec<Item>, cols: u8) {
    clamp(items, cols);
    items.sort_by_key(|i| (i.y, i.x));
    let mut placed: Vec<Item> = Vec::with_capacity(items.len());
    for mut it in items.drain(..) {
        if placed.iter().any(|p| p.overlaps(&it)) {
            (it.x, it.y) = free(&placed, it.w, it.h, cols);
        }
        placed.push(it);
    }
    *items = placed;
    compact(items, cols);
}

/// Items floated up as far as each goes, the higher ones (then the lefter) first.
pub fn compact(items: &mut [Item], cols: u8) {
    clamp(items, cols);
    let mut order: Vec<usize> = (0..items.len()).collect();
    order.sort_by_key(|&i| (items[i].y, items[i].x));
    for (n, &i) in order.iter().enumerate() {
        while items[i].y > 0 {
            let mut up = items[i].clone();
            up.y -= 1;
            if order[..n].iter().any(|&j| items[j].overlaps(&up)) {
                break;
            }
            items[i].y -= 1;
        }
    }
}

/// The item key put at (x, y) and size (w, h), those in its way pushed down below it (and those in theirs, on),
/// then the whole grid compacted. The moved item keeps its place over the others.
#[allow(clippy::too_many_arguments)]
pub fn place(items: &mut [Item], key: &str, x: u8, y: u8, w: u8, h: u8, cols: u8) {
    clamp(items, cols);
    let Some(m) = items.iter().position(|i| i.key == key) else { return };
    items[m].w = w.clamp(1, cols);
    items[m].h = h.max(1);
    items[m].x = x.min(cols - items[m].w);
    items[m].y = y;
    // the others pushed down in order from the top, each below whatever settled before it overlaps it
    let mut settled = vec![m];
    let mut order: Vec<usize> = (0..items.len()).filter(|&i| i != m).collect();
    order.sort_by_key(|&i| (items[i].y, items[i].x));
    for i in order {
        while let Some(&j) = settled.iter().find(|&&j| items[j].overlaps(&items[i])) {
            items[i].y = items[j].y + items[j].h;
        }
        settled.push(i);
    }
    // up again, the moved one first so it stays where it was put unless there is room above it
    let moved = items[m].clone();
    compact(items, cols);
    if let Some(it) = items.iter_mut().find(|i| i.key == moved.key) {
        it.x = moved.x;
    }
}

/// The first place, top to bottom and left to right, a w×h widget fits.
pub fn free(items: &[Item], w: u8, h: u8, cols: u8) -> (u8, u8) {
    let w = w.clamp(1, cols);
    for y in 0.. {
        for x in 0..=cols - w {
            let cand = Item { key: String::new(), x, y, w, h };
            if !items.iter().any(|i| i.overlaps(&cand)) {
                return (x, y);
            }
        }
    }
    unreachable!("an endless grid has room")
}

/// The first row past the band of rows from y to y + h and of every item reaching into it, so something put
/// there spans the grid's width without cutting through an item (an opened widget's menu goes there).
pub fn below(items: &[Item], y: u8, h: u8) -> u8 {
    let mut end = y + h;
    loop {
        let further = items.iter().filter(|i| i.y < end && i.y + i.h > y).map(|i| i.y + i.h).max().unwrap_or(end);
        if further <= end {
            return end;
        }
        end = further;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn it(key: &str, x: u8, y: u8, w: u8, h: u8) -> Item {
        Item { key: key.into(), x, y, w, h }
    }

    fn no_overlaps(items: &[Item]) -> bool {
        items.iter().enumerate().all(|(n, a)| items[n + 1..].iter().all(|b| !a.overlaps(b)))
    }

    #[test]
    fn sizes_fitted_and_layouts_rescaled() {
        // a toggle's sizes on 6: half, a third, a cell, the whole width
        assert_eq!(fit(&[(4, 1), (2, 1), (1, 1), (8, 1)], 6), [(3, 1), (2, 1), (1, 1), (6, 1)]);
        assert_eq!(fit(&[(4, 1), (8, 1)], 8), [(4, 1), (8, 1)]);
        assert_eq!((base(3, 6), base(6, 6), base(1, 6), base(2, 6)), (4, 8, 1, 3));
        let mut g = vec![it("wifi", 0, 0, 4, 1), it("bt", 4, 0, 4, 1), it("bright", 0, 1, 8, 1)];
        rescale(&mut g, 8, 6);
        assert_eq!(g, vec![it("wifi", 0, 0, 3, 1), it("bt", 3, 0, 3, 1), it("bright", 0, 1, 6, 1)]);
        rescale(&mut g, 6, 8);
        assert_eq!(g, vec![it("wifi", 0, 0, 4, 1), it("bt", 4, 0, 4, 1), it("bright", 0, 1, 8, 1)]);
        // an odd width: the halves split unevenly, side by side still, and back again
        rescale(&mut g, 8, 5);
        assert_eq!(g, vec![it("wifi", 0, 0, 3, 1), it("bt", 3, 0, 2, 1), it("bright", 0, 1, 5, 1)]);
        rescale(&mut g, 5, 8);
        assert_eq!(g, vec![it("wifi", 0, 0, 5, 1), it("bt", 5, 0, 3, 1), it("bright", 0, 1, 8, 1)]);
    }

    #[test]
    fn compact_floats_up() {
        let mut g = vec![it("a", 0, 2, 4, 1), it("b", 4, 3, 4, 1), it("c", 0, 4, 8, 1)];
        compact(&mut g, 8);
        assert_eq!(g, vec![it("a", 0, 0, 4, 1), it("b", 4, 0, 4, 1), it("c", 0, 1, 8, 1)]);
    }

    #[test]
    fn place_pushes_down_and_compacts() {
        // a full-width slider dropped on the first row pushes the two toggles below it
        let mut g = vec![it("wifi", 0, 0, 4, 1), it("bt", 4, 0, 4, 1), it("bright", 0, 1, 8, 1)];
        place(&mut g, "bright", 0, 0, 8, 1, 8);
        assert_eq!(g, vec![it("wifi", 0, 1, 4, 1), it("bt", 4, 1, 4, 1), it("bright", 0, 0, 8, 1)]);
        assert!(no_overlaps(&g));
    }

    #[test]
    fn place_cascades_and_resizes() {
        let mut g = vec![it("a", 0, 0, 2, 2), it("b", 2, 0, 2, 1), it("c", 2, 1, 2, 1), it("d", 0, 2, 8, 1)];
        // b grown to 4×2 where it is: c goes under it, d under both
        place(&mut g, "b", 2, 0, 4, 2, 8);
        assert!(no_overlaps(&g));
        assert_eq!(g.iter().find(|i| i.key == "b").unwrap(), &it("b", 2, 0, 4, 2));
        assert_eq!(g.iter().find(|i| i.key == "c").unwrap().y, 2);
        assert!(g.iter().find(|i| i.key == "d").unwrap().y >= 2);
    }

    #[test]
    fn place_keeps_within_width() {
        let mut g = vec![it("a", 0, 0, 2, 1)];
        place(&mut g, "a", 7, 0, 4, 1, 8);
        assert_eq!(g[0], it("a", 4, 0, 4, 1));
    }

    #[test]
    fn free_finds_the_first_hole() {
        let g = vec![it("a", 0, 0, 4, 1), it("b", 0, 1, 8, 1)];
        assert_eq!(free(&g, 4, 1, 8), (4, 0));
        assert_eq!(free(&g, 8, 1, 8), (0, 2));
    }

    #[test]
    fn a_file_with_overlaps_is_settled() {
        let mut g = vec![it("weather", 4, 0, 4, 2), it("media", 0, 0, 4, 2), it("month", 4, 0, 4, 5), it("clock", 0, 2, 4, 1)];
        settle(&mut g, 8);
        assert!(no_overlaps(&g), "{g:?}");
        assert_eq!(g.len(), 4);
    }

    #[test]
    fn moving_a_tile_anywhere_never_overlaps() {
        let start = vec![it("media", 0, 0, 4, 2), it("notifications", 0, 2, 4, 6), it("month", 4, 0, 4, 5), it("agenda", 4, 5, 4, 3)];
        for key in ["media", "notifications", "month", "agenda"] {
            let (w, h) = start.iter().find(|i| i.key == key).map(|i| (i.w, i.h)).unwrap();
            for x in 0..=(BASE - w) {
                for y in 0..10 {
                    let mut g = start.clone();
                    place(&mut g, key, x, y, w, h, 8);
                    assert!(no_overlaps(&g), "{key} to {x},{y}: {g:?}");
                }
            }
        }
    }

    #[test]
    fn below_skips_tall_neighbours() {
        // a 1-high toggle beside a 2-high player: its menu goes under the player
        let g = vec![it("wifi", 0, 0, 4, 1), it("player", 4, 0, 4, 2), it("bright", 0, 2, 8, 1)];
        assert_eq!(below(&g, 0, 1), 2);
        assert_eq!(below(&g, 2, 1), 3);
    }
}
