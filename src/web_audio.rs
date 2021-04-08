use std::{cell::RefCell, collections::HashMap, rc::Rc};

use bevy::{
    asset::{AssetLoader, BoxedFuture, HandleId, LoadContext, LoadedAsset},
    core::AsBytes,
    prelude::*,
    reflect::TypeUuid,
};
use js_sys::Uint8Array;
use wasm_bindgen::{prelude::Closure, JsCast, JsValue};
use web_sys::{AudioBuffer, AudioContext};

#[derive(Clone, Debug, TypeUuid)]
#[uuid = "90b42b22-96ee-11eb-b33d-00155d8e5904"]
pub struct Buffer(Vec<u8>);

#[derive(Clone, Copy, Debug, Default)]
pub struct BufferAssetLoader;

impl AssetLoader for BufferAssetLoader {
    fn load<'a>(
        &'a self,
        bytes: &'a [u8],
        load_context: &'a mut LoadContext,
    ) -> BoxedFuture<'a, Result<(), anyhow::Error>> {
        let bytes = bytes.to_vec();
        Box::pin(async move {
            load_context.set_default_asset(LoadedAsset::new(Buffer(bytes)));
            Ok(())
        })
    }

    fn extensions(&self) -> &[&str] {
        &["flac", "ogg", "wav"]
    }
}

#[derive(Clone, Debug, Default)]
struct Buffers(Rc<RefCell<HashMap<HandleId, AudioBuffer>>>);

fn buffer_creation(
    context: NonSend<AudioContext>,
    mut events: EventReader<AssetEvent<Buffer>>,
    assets: Res<Assets<Buffer>>,
    buffers: NonSend<Buffers>,
) {
    for event in events.iter() {
        match event {
            AssetEvent::Created { handle } => {
                if let Some(buffer) = assets.get(handle) {
                    let bytes = buffer.0.as_bytes();
                    let array: Uint8Array = bytes.into();
                    let array_buffer = array.buffer();
                    let handle_id = handle.id;
                    let buffers = buffers.0.clone();
                    let callback = Closure::wrap(Box::new(move |v: JsValue| {
                        let b: AudioBuffer = v.dyn_into().unwrap();
                        buffers.borrow_mut().insert(handle_id, b);
                    }) as Box<dyn FnMut(_)>);
                    context
                        .decode_audio_data(&array_buffer)
                        .unwrap()
                        .then(&callback);
                }
            }
            AssetEvent::Modified { handle: _ } => {}
            AssetEvent::Removed { handle } => {
                buffers.0.borrow_mut().remove(&handle.id);
            }
        }
    }
}

#[derive(Clone, Debug, Default, Reflect)]
#[reflect(Component)]
pub struct Sound {
    pub buffer: Handle<Buffer>,
}

#[derive(Clone, Copy, Default, Debug, Reflect)]
#[reflect(Component)]
pub struct Listener;

fn update_listener(
    context: NonSend<AudioContext>,
    listener: Query<(&Listener, Option<&Transform>)>,
) {
    let audio_listener = context.listener();
    let mut reset_listener = false;
    if let Ok((_, transform)) = listener.single() {
        if let Some(transform) = transform {
            let translation = transform.translation;
            audio_listener.set_position(
                translation.x.into(),
                translation.y.into(),
                translation.z.into(),
            );
        } else {
            reset_listener = true;
        }
    } else {
        reset_listener = true;
    }
    if reset_listener {
        audio_listener.set_position(0., 0., 0.);
        audio_listener.set_orientation(0., 0., -1., 0., 1., 0.);
        audio_listener.set_velocity(0., 0., 0.);
    }
}

pub struct WebAudioPlugin;

impl Plugin for WebAudioPlugin {
    fn build(&self, app: &mut AppBuilder) {
        let context = AudioContext::new().unwrap();
        app.add_asset::<Buffer>()
            .init_asset_loader::<BufferAssetLoader>()
            .init_non_send_resource::<Buffers>()
            .insert_non_send_resource(context)
            .register_type::<Sound>()
            .register_type::<Listener>()
            .add_system(buffer_creation.system())
            .add_system(update_listener.system());
    }
}
