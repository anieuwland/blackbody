// Perceptually uniform sequential
mod cividis;
pub use cividis::CIVIDIS;
mod inferno;
pub use inferno::INFERNO;
mod magma;
pub use magma::MAGMA;
mod plasma;
pub use plasma::PLASMA;
mod turbo;
pub use turbo::TURBO;
mod viridis;
pub use viridis::VIRIDIS;

// Sequential 1
mod blues;
pub use blues::BLUES;
mod bugn;
pub use bugn::BUGN;
mod bupu;
pub use bupu::BUPU;
mod gnbu;
pub use gnbu::GNBU;
mod greens;
pub use greens::GREENS;
mod greys;
pub use greys::GREYS;
mod oranges;
pub use oranges::ORANGES;
mod orrd;
pub use orrd::ORRD;
mod rdpu;
pub use rdpu::RDPU;
mod reds;
pub use reds::REDS;
mod pubu;
pub use pubu::PUBU;
mod pubugn;
pub use pubugn::PUBUGN;
mod ylgn;
pub use ylgn::YLGN;
mod ylgnbu;
pub use ylgnbu::YLGNBU;
mod ylorbr;
pub use ylorbr::YLORBR;
mod ylorrd;
pub use ylorrd::YLORRD;

// Sequential 2
mod afmhot;
pub use afmhot::AFMHOT;
mod bone;
pub use bone::BONE;
mod copper;
pub use copper::COPPER;
mod gray;
pub use gray::GRAY;
mod gist_heat;
pub use gist_heat::GIST_HEAT;
mod hot;
pub use hot::HOT;

// Cyclic
mod hsv;
pub use hsv::HSV;

// Diverging
mod coolwarm;
pub use coolwarm::COOLWARM;
mod spectral;
pub use spectral::SPECTRAL;

// Miscellaneous
mod brg;
pub use brg::BRG;
mod jet;
pub use jet::JET;
mod ocean;
pub use ocean::OCEAN;
mod terrain;
pub use terrain::TERRAIN;
