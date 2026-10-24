//! Tile pipeline: fetches basemap vector tiles (OpenFreeMap) and elevation tiles (Terrarium)
//! over the network with a disk cache, reads the aero / airspace / obstacle tiles from the
//! installed data pack, decodes them, renders chart tiles on worker threads and keeps the
//! resulting textures. Everything is driven from the UI thread once per frame.

use super::mvt::{self, Tile};
use super::pmtiles::PmTiles;
use super::render::{self, Dem, Inputs, Look, Src};
use super::symbols::{self, Sym};
use parking_lot::{Mutex, RwLock};
use std::collections::{HashMap, HashSet};
use std::hash::Hash;
use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::Arc;
use std::time::{Duration, Instant};

pub type Key = (u8, u32, u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Source {
    Base,
    Aero,
    Airspace,
    Obstacles,
}

impl Source {
    pub fn max_zoom(self) -> u8 {
        match self {
            Source::Base => 14,
            Source::Aero => 13,
            Source::Airspace => 11,
            Source::Obstacles => 12,
        }
    }
    pub fn min_zoom(self) -> u8 {
        match self {
            Source::Base => 0,
            Source::Aero => 3,
            Source::Airspace => 4,
            Source::Obstacles => 8,
        }
    }
}

pub struct VecTile {
    pub tile: Arc<Tile>,
    pub syms: Vec<Sym>,
}

enum Slot<T> {
    Loading,
    Ready(T),
    Empty,
    Failed(Instant),
}

struct Store<K, V> {
    map: HashMap<K, (Slot<V>, u64)>,
    cap: usize,
}

impl<K: Hash + Eq + Clone, V: Clone> Store<K, V> {
    fn new(cap: usize) -> Self {
        Store { map: HashMap::new(), cap }
    }
    /// Some(Some(v)) ready, Some(None) resolved empty / failed recently, None not there yet.
    fn get(&mut self, k: &K, frame: u64) -> Option<Option<V>> {
        let e = self.map.get_mut(k)?;
        e.1 = frame;
        match &e.0 {
            Slot::Ready(v) => Some(Some(v.clone())),
            Slot::Empty => Some(None),
            Slot::Failed(at) if at.elapsed() < Duration::from_secs(30) => Some(None),
            Slot::Failed(_) => None,
            Slot::Loading => None,
        }
    }
    fn needs_fetch(&self, k: &K) -> bool {
        match self.map.get(k) {
            None => true,
            Some((Slot::Failed(at), _)) => at.elapsed() >= Duration::from_secs(30),
            _ => false,
        }
    }
    fn set(&mut self, k: K, s: Slot<V>, frame: u64) {
        self.map.insert(k, (s, frame));
        if self.map.len() > self.cap {
            let mut old: Vec<(K, u64)> = self.map.iter().filter(|(_, (s, _))| !matches!(s, Slot::Loading)).map(|(k, (_, f))| (k.clone(), *f)).collect();
            old.sort_by_key(|x| x.1);
            for (k, _) in old.into_iter().take(self.map.len() - self.cap * 3 / 4) {
                self.map.remove(&k);
            }
        }
    }
}

struct Archives {
    aero: PmTiles,
    airspace: PmTiles,
    obstacles: PmTiles,
}

struct Rendered {
    key: Key,
    rev: u64,
    mask: u8,
    image: egui::ColorImage,
}

pub struct Tex {
    pub tex: egui::TextureHandle,
    rev: u64,
    mask: u8,
    used: u64,
}

pub struct Tiles {
    rt: tokio::runtime::Handle,
    http: reqwest::Client,
    cache_dir: PathBuf,
    tiles_dir: PathBuf,
    archives: Arc<RwLock<Option<Arc<Archives>>>>,
    base_template: Arc<Mutex<Option<String>>>,
    base_template_loading: bool,
    vec: Arc<Mutex<Store<(Source, u8, u32, u32), Arc<VecTile>>>>,
    dem: Arc<Mutex<Store<Key, Arc<Dem>>>>,
    tx: mpsc::Sender<Rendered>,
    rx: mpsc::Receiver<Rendered>,
    textures: HashMap<Key, Tex>,
    rendering: HashSet<Key>,
    pub frame: u64,
    ctx: egui::Context,
    /// offline: stop asking the network for a while after failures
    pub net_errors: Arc<Mutex<u32>>,
}

const MASK_BASE: u8 = 1;
const MASK_DEM: u8 = 2;
const MASK_AERO: u8 = 4;
const MASK_AIRSPACE: u8 = 8;

impl Tiles {
    pub fn new(ctx: egui::Context, rt: tokio::runtime::Handle, http: reqwest::Client, cache_dir: PathBuf, tiles_dir: PathBuf) -> Self {
        let (tx, rx) = mpsc::channel();
        let t = Tiles {
            rt,
            http,
            cache_dir,
            tiles_dir,
            archives: Arc::new(RwLock::new(None)),
            base_template: Arc::new(Mutex::new(None)),
            base_template_loading: false,
            vec: Arc::new(Mutex::new(Store::new(600))),
            dem: Arc::new(Mutex::new(Store::new(300))),
            tx,
            rx,
            textures: HashMap::new(),
            rendering: HashSet::new(),
            frame: 0,
            ctx,
            net_errors: Arc::new(Mutex::new(0)),
        };
        t.reload_archives();
        t
    }

    /// (Re)open the data pack's tile archives, e.g. after a new pack was installed.
    pub fn reload_archives(&self) {
        let open = || -> anyhow::Result<Archives> {
            Ok(Archives {
                aero: PmTiles::open(&self.tiles_dir.join("aero.pmtiles"))?,
                airspace: PmTiles::open(&self.tiles_dir.join("airspace.pmtiles"))?,
                obstacles: PmTiles::open(&self.tiles_dir.join("obstacles.pmtiles"))?,
            })
        };
        *self.archives.write() = open().ok().map(Arc::new);
        // forget everything read from the old files
        self.vec.lock().map.retain(|k, _| k.0 == Source::Base);
    }

    pub fn has_data(&self) -> bool {
        self.archives.read().is_some()
    }

    /// Drop all rendered textures (theme / layers / altitude filter changed).
    pub fn invalidate(&mut self) {
        // keep textures so the old look shows until the new one is ready; revs differ now
    }

    pub fn begin_frame(&mut self) {
        self.frame += 1;
        while let Ok(r) = self.rx.try_recv() {
            self.rendering.remove(&r.key);
            let name = format!("tile-{}-{}-{}", r.key.0, r.key.1, r.key.2);
            match self.textures.get_mut(&r.key) {
                Some(t) => {
                    t.tex.set(r.image, egui::TextureOptions::LINEAR);
                    t.rev = r.rev;
                    t.mask = r.mask;
                }
                None => {
                    let tex = self.ctx.load_texture(name, r.image, egui::TextureOptions::LINEAR);
                    self.textures.insert(r.key, Tex { tex, rev: r.rev, mask: r.mask, used: self.frame });
                }
            }
        }
        if self.textures.len() > 160 {
            let mut v: Vec<(Key, u64)> = self.textures.iter().map(|(k, t)| (*k, t.used)).collect();
            v.sort_by_key(|x| x.1);
            for (k, _) in v.into_iter().take(40) {
                self.textures.remove(&k);
            }
        }
        if self.base_template.lock().is_none() && !self.base_template_loading {
            self.base_template_loading = true;
            let http = self.http.clone();
            let slot = self.base_template.clone();
            let cache = self.cache_dir.join("openfreemap-tilejson.json");
            let ctx = self.ctx.clone();
            self.rt.spawn(async move {
                let fetched = async {
                    let v: serde_json::Value = http.get("https://tiles.openfreemap.org/planet").send().await?.error_for_status()?.json().await?;
                    let url = v["tiles"][0].as_str().unwrap_or_default().to_string();
                    anyhow::ensure!(url.contains("{z}"), "bad tilejson");
                    let _ = tokio::fs::create_dir_all(cache.parent().unwrap()).await;
                    let _ = tokio::fs::write(&cache, &url).await;
                    Ok::<_, anyhow::Error>(url)
                }
                .await;
                // offline: use the last known template
                let url = match fetched {
                    Ok(u) => Some(u),
                    Err(_) => tokio::fs::read_to_string(&cache).await.ok(),
                };
                *slot.lock() = url;
                ctx.request_repaint();
            });
        }
    }

    /// Vector tile for a source at (z, x, y), fetching it if needed. Some(None) = no data there.
    pub fn vector(&mut self, src: Source, z: u8, x: u32, y: u32) -> Option<Option<Arc<VecTile>>> {
        let key = (src, z, x, y);
        let frame = self.frame;
        let mut store = self.vec.lock();
        if let Some(v) = store.get(&key, frame) {
            return Some(v);
        }
        if !store.needs_fetch(&key) {
            return None;
        }
        if src != Source::Base && self.archives.read().is_none() {
            store.set(key, Slot::Empty, frame);
            return Some(None);
        }
        let template = if src == Source::Base {
            match self.base_template.lock().clone() {
                Some(t) => Some(t),
                None => return None,
            }
        } else {
            None
        };
        store.set(key, Slot::Loading, frame);
        drop(store);
        let vec = self.vec.clone();
        let archives = self.archives.read().clone();
        let http = self.http.clone();
        let cache = self.cache_dir.clone();
        let ctx = self.ctx.clone();
        let errors = self.net_errors.clone();
        self.rt.spawn(async move {
            let bytes: anyhow::Result<Option<Vec<u8>>> = match (src, template) {
                (Source::Base, Some(t)) => {
                    let version = t.split('/').rev().nth(4).unwrap_or("planet").to_string();
                    let path = cache.join("base").join(version).join(format!("{z}/{x}/{y}.pbf"));
                    let url = t.replace("{z}", &z.to_string()).replace("{x}", &x.to_string()).replace("{y}", &y.to_string());
                    cached_get(&http, &url, &path, &errors).await
                }
                _ => {
                    let a = archives.clone();
                    tokio::task::spawn_blocking(move || {
                        let a = a.ok_or_else(|| anyhow::anyhow!("no data"))?;
                        match src {
                            Source::Aero => a.aero.get(z, x, y),
                            Source::Airspace => a.airspace.get(z, x, y),
                            _ => a.obstacles.get(z, x, y),
                        }
                    })
                    .await
                    .unwrap_or_else(|e| Err(anyhow::anyhow!("{e}")))
                }
            };
            let slot = match bytes {
                Ok(Some(b)) => {
                    let decoded = tokio::task::spawn_blocking(move || {
                        let b = if b.starts_with(&[0x1f, 0x8b]) { gunzip(&b).unwrap_or_default() } else { b };
                        let tile = mvt::decode(&b);
                        let syms = symbols::extract(src, z, x, y, &tile);
                        VecTile { tile: Arc::new(tile), syms }
                    })
                    .await;
                    match decoded {
                        Ok(v) => Slot::Ready(Arc::new(v)),
                        Err(_) => Slot::Failed(Instant::now()),
                    }
                }
                Ok(None) => Slot::Empty,
                Err(_) => Slot::Failed(Instant::now()),
            };
            vec.lock().set(key, slot, 0);
            ctx.request_repaint();
        });
        None
    }

    /// Elevation tile; Some(None) = unavailable.
    fn dem(&mut self, z: u8, x: u32, y: u32) -> Option<Option<Arc<Dem>>> {
        let key = (z, x, y);
        let frame = self.frame;
        let mut store = self.dem.lock();
        if let Some(v) = store.get(&key, frame) {
            return Some(v);
        }
        if !store.needs_fetch(&key) {
            return None;
        }
        store.set(key, Slot::Loading, frame);
        drop(store);
        let dem = self.dem.clone();
        let http = self.http.clone();
        let path = self.cache_dir.join("dem").join(format!("{z}/{x}/{y}.png"));
        let ctx = self.ctx.clone();
        let errors = self.net_errors.clone();
        self.rt.spawn(async move {
            let url = format!("https://s3.amazonaws.com/elevation-tiles-prod/terrarium/{z}/{x}/{y}.png");
            let slot = match cached_get(&http, &url, &path, &errors).await {
                Ok(Some(b)) => match tokio::task::spawn_blocking(move || Dem::from_png(z, &b)).await {
                    Ok(Ok(d)) => Slot::Ready(Arc::new(d)),
                    _ => Slot::Empty,
                },
                Ok(None) => Slot::Empty,
                Err(_) => Slot::Failed(Instant::now()),
            };
            dem.lock().set(key, slot, 0);
            ctx.request_repaint();
        });
        None
    }

    fn ancestor(z: u8, x: u32, y: u32, zs: u8) -> (u8, u32, u32) {
        let dz = z - zs;
        (zs, x >> dz, y >> dz)
    }

    /// The chart texture for a tile: makes sure its inputs are loading and a render is queued
    /// when something changed. Returns whatever texture is current (possibly an older look).
    pub fn tile(&mut self, key: Key, look: &Look, rev: u64) -> Option<egui::TextureId> {
        let (z, x, y) = key;
        let mut mask_needed = 0u8;
        let mut mask_have = 0u8;
        let base_z = z.min(Source::Base.max_zoom());
        let (bz, bx, by) = Self::ancestor(z, x, y, base_z);
        mask_needed |= MASK_BASE;
        let base = self.vector(Source::Base, bz, bx, by);
        if base.is_some() {
            mask_have |= MASK_BASE;
        }
        let terrain = look.layers.on(super::style::Group::Terrain);
        let mut dem = None;
        if terrain {
            mask_needed |= MASK_DEM;
            let (dz, dx, dy) = Self::ancestor(z, x, y, z.min(12));
            let d = self.dem(dz, dx, dy);
            if d.is_some() {
                mask_have |= MASK_DEM;
            }
            dem = d.flatten().map(|d| Src { data: d, z: dz, x: dx, y: dy });
        }
        let mut aero = None;
        if z >= Source::Aero.min_zoom() {
            mask_needed |= MASK_AERO;
            let (az, ax, ay) = Self::ancestor(z, x, y, z.min(Source::Aero.max_zoom()));
            let a = self.vector(Source::Aero, az, ax, ay);
            if a.is_some() {
                mask_have |= MASK_AERO;
            }
            aero = a.flatten().map(|v| Src { data: v.tile.clone(), z: az, x: ax, y: ay });
        }
        let mut airspace = None;
        if z >= Source::Airspace.min_zoom() {
            mask_needed |= MASK_AIRSPACE;
            let (sz, sx, sy) = Self::ancestor(z, x, y, z.min(Source::Airspace.max_zoom()));
            let a = self.vector(Source::Airspace, sz, sx, sy);
            if a.is_some() {
                mask_have |= MASK_AIRSPACE;
            }
            airspace = a.flatten().map(|v| Src { data: v.tile.clone(), z: sz, x: sx, y: sy });
        }
        let base = base.flatten().map(|v| Src { data: v.tile.clone(), z: bz, x: bx, y: by });

        let frame = self.frame;
        let current = self.textures.get_mut(&key).map(|t| {
            t.used = frame;
            (t.tex.id(), t.rev, t.mask)
        });
        let stale = match current {
            None => true,
            Some((_, r, m)) => r != rev || m != mask_have,
        };
        // render as soon as anything is in (base or terrain), again when more arrives
        if stale && mask_have != 0 && !self.rendering.contains(&key) {
            self.rendering.insert(key);
            let inputs = Inputs { z, x, y, base, aero, airspace, dem };
            let look = look.clone();
            let tx = self.tx.clone();
            let ctx = self.ctx.clone();
            self.rt.spawn_blocking(move || {
                let pm = render::render(&inputs, &look);
                let image = egui::ColorImage::from_rgba_premultiplied([pm.width() as usize, pm.height() as usize], pm.data());
                let _ = tx.send(Rendered { key, rev, mask: mask_have, image });
                ctx.request_repaint();
            });
        }
        let _ = mask_needed;
        current.map(|c| c.0)
    }

    /// Texture for a tile if one exists (for drawing parents under missing children).
    pub fn existing(&self, key: Key) -> Option<egui::TextureId> {
        self.textures.get(&key).map(|t| t.tex.id())
    }

    pub fn busy(&self) -> bool {
        !self.rendering.is_empty()
    }
}

fn gunzip(b: &[u8]) -> std::io::Result<Vec<u8>> {
    use std::io::Read;
    let mut out = Vec::new();
    flate2::read::GzDecoder::new(b).read_to_end(&mut out)?;
    Ok(out)
}

/// GET with a file cache. Ok(None) for 404 / 204 (no tile there).
async fn cached_get(http: &reqwest::Client, url: &str, path: &PathBuf, errors: &Mutex<u32>) -> anyhow::Result<Option<Vec<u8>>> {
    if let Ok(b) = tokio::fs::read(path).await {
        return Ok(if b.is_empty() { None } else { Some(b) });
    }
    let r = http.get(url).send().await;
    let r = match r {
        Ok(r) => r,
        Err(e) => {
            *errors.lock() += 1;
            return Err(e.into());
        }
    };
    let status = r.status();
    if status == reqwest::StatusCode::NOT_FOUND || status == reqwest::StatusCode::NO_CONTENT {
        let _ = tokio::fs::create_dir_all(path.parent().unwrap()).await;
        let _ = tokio::fs::write(path, b"").await;
        return Ok(None);
    }
    let b = r.error_for_status()?.bytes().await?.to_vec();
    *errors.lock() = 0;
    if let Some(dir) = path.parent() {
        let _ = tokio::fs::create_dir_all(dir).await;
    }
    let _ = tokio::fs::write(path, &b).await;
    Ok(Some(b))
}
