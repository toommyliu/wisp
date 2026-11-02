use super::{Location, LocationKind, Player, Position};

#[derive(Default)]
pub(super) struct Movement {
    observed: Option<(Location, Option<Position>)>,
    pub pending: Option<(Position, u64)>,
    revision: u64,
}
impl Location {
    pub(super) fn same(&self, other: &Self) -> bool {
        self.map.eq_ignore_ascii_case(&other.map)
            && self.cell.eq_ignore_ascii_case(&other.cell)
            && self.pad.eq_ignore_ascii_case(&other.pad)
    }
}
impl Position {
    fn finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite()
    }
}
impl Movement {
    pub fn seed(&mut self, target: &Player) {
        if self.observed.is_none() {
            self.observed = Some((
                target.location.clone(),
                target
                    .position
                    .filter(|p| p.finite() && (p.x != 0.0 || p.y != 0.0)),
            ));
            self.revision += 1;
        }
    }
    pub fn observe(&mut self, location: Location, kind: LocationKind, copy_walk: bool) -> bool {
        let changed = self
            .observed
            .as_ref()
            .is_none_or(|(old, _)| !old.same(&location));
        let position = match kind {
            LocationKind::Position(position) => {
                if !position.finite() {
                    return false;
                }
                Some(position)
            }
            LocationKind::Walk(position) => position.filter(|p| p.finite()),
            LocationKind::Cell => None,
        };
        let duplicate = self
            .observed
            .as_ref()
            .is_some_and(|(_, old)| *old == position);
        if !changed && (position.is_none() || duplicate) {
            return false;
        }
        self.revision += 1;
        if changed {
            self.pending = None;
        }
        let queue = copy_walk && matches!(kind, LocationKind::Walk(_)) && position.is_some();
        if queue {
            self.pending = position.map(|p| (p, self.revision));
        }
        self.observed = Some((location, position));
        changed || queue
    }
    pub fn complete(&mut self, revision: u64) {
        if self.pending.is_some_and(|(_, current)| revision == current) {
            self.pending = None;
        }
    }
}
