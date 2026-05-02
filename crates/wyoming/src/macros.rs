/// Generate a boilerplate [`Eventable`](crate::event::Eventable) impl
/// that round-trips through `serde_json::to_value` / `from_value`. Use
/// for events whose entire content lives in `Event.data` (no binary
/// payload). For events with a payload (e.g.
/// [`AudioChunk`](crate::audio::AudioChunk)), implement the trait
/// manually.
#[macro_export]
macro_rules! impl_eventable {
    ($ty:ty, $event_type:literal) => {
        impl $crate::event::Eventable for $ty {
            const EVENT_TYPE: &'static str = $event_type;

            fn into_event(self) -> $crate::event::Event {
                let value = ::serde_json::to_value(&self).expect("serializable struct");
                let map = match value {
                    ::serde_json::Value::Object(m) => m,
                    _ => ::serde_json::Map::new(),
                };
                $crate::event::Event::new(<Self as $crate::event::Eventable>::EVENT_TYPE)
                    .with_data(map)
            }

            fn from_event(event: &$crate::event::Event) -> $crate::error::Result<Self> {
                if event.event_type != <Self as $crate::event::Eventable>::EVENT_TYPE {
                    return Err($crate::error::Error::WrongType {
                        expected: <Self as $crate::event::Eventable>::EVENT_TYPE,
                        actual: event.event_type.clone(),
                    });
                }
                let value = ::serde_json::Value::Object(event.data.clone());
                ::serde_json::from_value(value).map_err(|source| $crate::error::Error::Schema {
                    event_type: <Self as $crate::event::Eventable>::EVENT_TYPE,
                    source,
                })
            }
        }
    };
}
