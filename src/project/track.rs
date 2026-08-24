use super::clip::Clip;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct TrackId(pub u32);

#[derive(Clone)]
pub struct Track {
    pub id: TrackId,
    pub name: String,
    /// Signed pan percentage, -100 (full left) ..= 100 (full right), in
    /// steps of 5. Kept as an integer so the UI's 5% steps are exact.
    pub pan_percent: i8,
    /// Linear gain multiplier, default 1.0.
    pub volume: f32,
    pub muted: bool,
    pub soloed: bool,
    pub clips: Vec<Clip>,
}

impl Track {
    pub fn new(id: TrackId, name: String) -> Self {
        Track {
            id,
            name,
            pan_percent: 0,
            volume: 1.0,
            muted: false,
            soloed: false,
            clips: Vec::new(),
        }
    }
}
