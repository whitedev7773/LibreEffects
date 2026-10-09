//! Keep generated SVG image references small. Media lives in bounded memory,
//! rather than repeatedly being copied through XML and decoded from Base64.
use base64::{Engine, engine::general_purpose::STANDARD};
use std::{
    cell::RefCell,
    collections::VecDeque,
    marker::PhantomData,
    rc::Rc,
    sync::{Arc, Mutex},
};

const CACHE_BYTES: usize = 32 * 1024 * 1024;
const FRAME_BYTES: usize = 128 * 1024 * 1024;
const PREFIX: &str = "libre-frame-image:";

#[derive(Default)]
pub(crate) struct Sources {
    entries: VecDeque<(Arc<str>, Arc<Vec<u8>>)>,
    bytes: usize,
}
impl Sources {
    pub(crate) fn clear(&mut self) {
        self.entries.clear();
        self.bytes = 0;
    }
    fn decode(&mut self, png: &str) -> Result<Arc<Vec<u8>>, String> {
        if let Some(index) = self
            .entries
            .iter()
            .position(|(source, _)| source.as_ref() == png)
        {
            let entry = self.entries.remove(index).unwrap();
            let bytes = entry.1.clone();
            self.entries.push_back(entry);
            return Ok(bytes);
        }
        if png.len() > crate::rendering::SVG_LIMIT {
            return Err("Source PNG exceeds the 64 MiB limit".into());
        }
        let data = Arc::new(STANDARD.decode(png).map_err(|e| e.to_string())?);
        let bytes = png.len() + data.len();
        if bytes <= CACHE_BYTES {
            while !self.entries.is_empty()
                && (self.bytes + bytes > CACHE_BYTES || self.entries.len() >= 128)
            {
                let (source, data) = self.entries.pop_front().unwrap();
                self.bytes -= source.len() + data.len();
            }
            self.bytes += bytes;
            self.entries.push_back((png.into(), data.clone()));
        }
        Ok(data)
    }
}
struct Frame {
    sources: Arc<Mutex<Sources>>,
    images: Vec<Arc<Vec<u8>>>,
    bytes: usize,
}
thread_local! { static FRAME: RefCell<Option<Frame>> = const { RefCell::new(None) }; }
pub(crate) struct Guard {
    previous: Option<Frame>,
    thread: PhantomData<Rc<()>>,
}
impl Drop for Guard {
    fn drop(&mut self) {
        FRAME.with(|slot| *slot.borrow_mut() = self.previous.take());
    }
}
pub(crate) fn begin(sources: Arc<Mutex<Sources>>) -> Guard {
    Guard {
        previous: FRAME.with(|slot| {
            slot.replace(Some(Frame {
                sources,
                images: Vec::new(),
                bytes: 0,
            }))
        }),
        thread: PhantomData,
    }
}
fn register(frame: &mut Frame, data: Arc<Vec<u8>>) -> Result<String, String> {
    if let Some(index) = frame.images.iter().position(|image| **image == *data) {
        return Ok(format!("{PREFIX}{index}"));
    }
    if frame.bytes.saturating_add(data.len()) > FRAME_BYTES || frame.images.len() >= 4096 {
        return Err("Frame image resources exceed the bounded 128 MiB budget".into());
    }
    let index = frame.images.len();
    frame.bytes += data.len();
    frame.images.push(data);
    Ok(format!("{PREFIX}{index}"))
}
pub(crate) fn href(png: &str) -> Result<String, String> {
    FRAME.with(|slot| {
        let mut slot = slot.borrow_mut();
        let Some(frame) = slot.as_mut() else {
            return Ok(format!("data:image/png;base64,{png}"));
        };
        let data = frame
            .sources
            .lock()
            .map_err(|_| "Source image cache is unavailable")?
            .decode(png)?;
        register(frame, data)
    })
}
pub(crate) fn encoded(data: Vec<u8>) -> Result<Option<String>, String> {
    FRAME.with(|slot| {
        let mut slot = slot.borrow_mut();
        slot.as_mut()
            .map(|frame| register(frame, Arc::new(data)))
            .transpose()
    })
}
pub(crate) fn resolver() -> resvg::usvg::ImageHrefStringResolverFn<'static> {
    let fallback = resvg::usvg::ImageHrefResolver::default_string_resolver();
    Box::new(move |href, options| {
        if let Some(index) = href.strip_prefix(PREFIX) {
            let index = index.parse::<usize>().ok()?;
            return FRAME.with(|slot| {
                slot.borrow()
                    .as_ref()?
                    .images
                    .get(index)
                    .cloned()
                    .map(resvg::usvg::ImageKind::PNG)
            });
        }
        fallback(href, options)
    })
}

/// Only resolved image bytes, including their association with each short URI,
/// can qualify an SVG intermediate for reuse. File/unknown hrefs stay uncached.
pub(crate) fn resources(svg: &str) -> Option<Vec<Arc<Vec<u8>>>> {
    let mut indices = std::collections::BTreeSet::new();
    for attribute in svg.split("href=").skip(1) {
        let quote = attribute.chars().next()?;
        if !matches!(quote, '\'' | '"') {
            return None;
        }
        let value = attribute.get(1..)?.split(quote).next()?;
        if let Some(index) = value.strip_prefix(PREFIX) {
            indices.insert(index.parse::<usize>().ok()?);
        } else if !value.starts_with("data:") {
            return None;
        }
    }
    if indices.is_empty() {
        return Some(Vec::new());
    }
    FRAME.with(|slot| {
        let slot = slot.borrow();
        let frame = slot.as_ref()?;
        indices
            .into_iter()
            .map(|index| frame.images.get(index).cloned())
            .collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn short_references_reuse_bytes_and_never_outlive_or_leak_between_frames() {
        let sources = Arc::new(Mutex::new(Sources::default()));
        let options = resvg::usvg::Options::default();
        let resolve = resolver();
        let href_a;
        {
            let _frame = begin(sources.clone());
            href_a = href("AQID").unwrap();
            assert_eq!(href("AQID").unwrap(), href_a);
            let resvg::usvg::ImageKind::PNG(bytes) = resolve(&href_a, &options).unwrap() else {
                panic!();
            };
            assert_eq!(*bytes, [1, 2, 3]);
            {
                let _child = begin(sources.clone());
                let resvg::usvg::ImageKind::PNG(child) =
                    resolve(&href("BAUG").unwrap(), &options).unwrap()
                else {
                    panic!();
                };
                assert_eq!(*child, [4, 5, 6]);
            }
            let resvg::usvg::ImageKind::PNG(restored) = resolve(&href_a, &options).unwrap() else {
                panic!();
            };
            assert!(Arc::ptr_eq(&bytes, &restored));
            std::thread::spawn(move || {
                assert!(resolver()(&href_a, &resvg::usvg::Options::default()).is_none())
            })
            .join()
            .unwrap();
        }
        assert!(resolve("libre-frame-image:0", &options).is_none());
        assert_eq!(href("AQID").unwrap(), "data:image/png;base64,AQID");
        assert_eq!(sources.lock().unwrap().entries.len(), 2);
        sources.lock().unwrap().clear();
        assert_eq!(sources.lock().unwrap().bytes, 0);
    }

    #[test]
    fn intermediate_keys_include_only_referenced_media_and_frame_budget_rejects_growth() {
        let sources = Arc::new(Mutex::new(Sources::default()));
        let _frame = begin(sources.clone());
        let first = href("AQID").unwrap();
        href("BAUG").unwrap();
        let used = resources(&format!("<image xlink:href='{first}'/>")).unwrap();
        assert_eq!(used.len(), 1);
        assert_eq!(*used[0], [1, 2, 3]);
        assert!(resources("<image href='C:/changed.png'/>").is_none());
        assert!(resources("<image href='libre-frame-image:900'/>").is_none());
        assert!(resources("<rect width='4'/>").unwrap().is_empty());
        let mut frame = Frame {
            sources,
            images: vec![],
            bytes: FRAME_BYTES,
        };
        assert!(register(&mut frame, Arc::new(vec![7])).is_err());
        assert!(frame.images.is_empty());
    }
}
