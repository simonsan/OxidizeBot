use crate::web::EMPTY;
use failure::bail;
use parking_lot::{MappedRwLockReadGuard, RwLock, RwLockReadGuard};
use std::sync::Arc;
use warp::{body, filters, path, Filter as _};

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct DeleteRequest {
    ns: Option<serde_json::Value>,
    key: serde_json::Value,
}

/// Cache endpoints.
#[derive(Clone)]
pub struct Cache(Arc<RwLock<Option<crate::storage::Cache>>>);

impl Cache {
    pub fn route(
        cache: Arc<RwLock<Option<crate::storage::Cache>>>,
    ) -> filters::BoxedFilter<(impl warp::Reply,)> {
        let api = Cache(cache);

        let list = warp::get2()
            .and(path::end().and_then({
                let api = api.clone();
                move || {
                    let api = api.clone();
                    async move { api.list().map_err(warp::reject::custom) }
                }
            }))
            .boxed();

        let delete = warp::delete2()
            .and(path::end().and(body::json()).and_then({
                let api = api.clone();
                move |body: DeleteRequest| {
                    let api = api.clone();
                    async move { api.delete(body).map_err(warp::reject::custom) }
                }
            }))
            .boxed();

        return warp::path("cache").and(list.or(delete)).boxed();
    }

    /// Access underlying cache abstraction.
    fn cache(&self) -> Result<MappedRwLockReadGuard<'_, crate::storage::Cache>, failure::Error> {
        match RwLockReadGuard::try_map(self.0.read(), |c| c.as_ref()) {
            Ok(out) => Ok(out),
            Err(_) => bail!("cache not configured"),
        }
    }

    /// List all cache entries.
    fn list(&self) -> Result<impl warp::Reply, failure::Error> {
        let entries = self.cache()?.list_json()?;
        Ok(warp::reply::json(&entries))
    }

    /// Delete a cache entry.
    fn delete(&self, request: DeleteRequest) -> Result<impl warp::Reply, failure::Error> {
        self.cache()?
            .delete_with_ns(request.ns.as_ref(), &request.key)?;
        Ok(warp::reply::json(&EMPTY))
    }
}
