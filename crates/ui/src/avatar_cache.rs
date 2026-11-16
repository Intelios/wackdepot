//! The app-global avatar image cache: a gpui `ImageCache` that answers
//! `img(Resource::Uri)` for avatar URLs (built by
//! [`wackdepot_github::avatar_url`]). Modelled on gpui's own
//! `RetainAllImageCache` — dedupe in-flight requests, notify the current
//! view when a load lands — plus Desktop's failure rule: a URL that
//! errored isn't retried for five minutes, so scrolling a long history
//! can't hot-loop a down CDN.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures::FutureExt as _;
use futures::future::Shared;
use gpui::{
    App, AppContext, Entity, Global, ImageCache, ImageCacheError, RenderImage, Resource, Task,
    Window,
};
use wackdepot_github::http::{HttpTransport, Transport};

/// How long a failed fetch stands down before we try again — Desktop's
/// `FailingAvatars`/`RetryLimit` rule.
const RETRY_LIMIT: Duration = Duration::from_secs(5 * 60);

/// Most URLs held at once; past this the oldest ready entries are
/// evicted (failures go first).
const CACHE_CAP: usize = 512;

enum Item {
    Loading(Shared<Task<Result<Arc<RenderImage>, ImageCacheError>>>),
    Ready(Arc<RenderImage>, Instant),
    Failed(ImageCacheError, Instant),
}

/// What `load` learned about a URL before it decides whether to fetch.
enum Status {
    InFlight,
    Ready(Arc<RenderImage>),
    FailedRecent(ImageCacheError),
    Fetch,
}

pub struct AvatarStore {
    transport: Arc<dyn Transport>,
    items: HashMap<String, Item>,
}

struct SharedAvatars(Entity<AvatarStore>);
impl Global for SharedAvatars {}

pub fn init(cx: &mut App) {
    let entity = cx.new(|_| AvatarStore {
        transport: Arc::new(HttpTransport::default()),
        items: HashMap::new(),
    });
    cx.observe_release(&entity, |store, cx| {
        for (_, item) in std::mem::take(&mut store.items) {
            if let Item::Ready(image, _) = item {
                cx.drop_image(image, None);
            }
        }
    })
    .detach();
    cx.set_global(SharedAvatars(entity));
}

/// The shared store for `.image_cache(…)` on avatar `img`s.
pub fn store(cx: &App) -> Entity<AvatarStore> {
    cx.global::<SharedAvatars>().0.clone()
}

impl ImageCache for AvatarStore {
    fn load(
        &mut self,
        resource: &Resource,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<Result<Arc<RenderImage>, ImageCacheError>> {
        let Resource::Uri(uri) = resource else {
            return None;
        };
        let now = Instant::now();
        let status = match self.items.get_mut(uri.as_str()) {
            Some(Item::Loading(task)) => match task.clone().now_or_never() {
                Some(result) => {
                    self.items.insert(
                        uri.to_string(),
                        match result.clone() {
                            Ok(image) => Item::Ready(image, now),
                            Err(error) => Item::Failed(error, now),
                        },
                    );
                    return Some(result);
                }
                None => Status::InFlight,
            },
            Some(Item::Ready(image, _)) => Status::Ready(image.clone()),
            Some(Item::Failed(error, at)) if now.duration_since(*at) < RETRY_LIMIT => {
                Status::FailedRecent(error.clone())
            }
            _ => Status::Fetch,
        };
        match status {
            Status::InFlight => None,
            Status::Ready(image) => Some(Ok(image)),
            Status::FailedRecent(error) => Some(Err(error)),
            Status::Fetch => {
                self.begin_fetch(uri.as_str(), window, cx);
                self.evict(window, cx);
                None
            }
        }
    }
}

impl AvatarStore {
    /// Kick off a fetch + decode on the background executor and notify
    /// the view currently rendering when it lands, mirroring
    /// `RetainAllImageCache`.
    fn begin_fetch(&mut self, uri: &str, window: &mut Window, cx: &mut App) {
        let url = uri.to_string();
        let transport = self.transport.clone();
        let key = url.clone();
        let task = cx
            .background_executor()
            .spawn(async move {
                let bytes = fetch_bytes(transport.as_ref(), &url)?;
                crate::model::image::render_image(&bytes).ok_or_else(|| {
                    ImageCacheError::Asset("GitHub didn't send back an image.".into())
                })
            })
            .shared();
        self.items.insert(key, Item::Loading(task.clone()));

        let view = window.current_view();
        window
            .spawn(cx, {
                async move |cx| {
                    let _ = task.await;
                    cx.on_next_frame(move |_, cx| {
                        cx.notify(view);
                    });
                }
            })
            .detach();
    }

    /// Keep the cache bounded: failures first, then the oldest ready.
    fn evict(&mut self, window: &mut Window, cx: &mut App) {
        if self.items.len() > CACHE_CAP {
            self.items
                .retain(|_, item| !matches!(item, Item::Failed(_, _)));
        }
        while self.items.len() > CACHE_CAP {
            let Some(oldest) = self
                .items
                .iter()
                .filter_map(|(u, i)| match i {
                    Item::Ready(_, at) => Some((u.clone(), *at)),
                    _ => None,
                })
                .min_by_key(|(_, at)| *at)
                .map(|(u, _)| u)
            else {
                break;
            };
            if let Some(Item::Ready(image, _)) = self.items.remove(&oldest) {
                cx.drop_image(image, Some(&mut *window));
            }
        }
    }
}

fn fetch_bytes(transport: &dyn Transport, url: &str) -> Result<Vec<u8>, ImageCacheError> {
    match wackdepot_github::fetch_avatar(transport, url) {
        Ok(bytes) => Ok(bytes),
        Err(_) => Err(ImageCacheError::Asset(
            "Couldn't reach GitHub for this avatar.".into(),
        )),
    }
}
