use super::super::Game;
use super::scrub_timeline::SoundRecord;
use horizon_client::fast_hash::FxHashMap;
use horizon_core::random::LegacyRandom;
use std::{
    collections::VecDeque,
    io::Cursor,
    sync::{mpsc, Arc, Mutex, OnceLock},
};

const MAX_AUDIO_CACHE_ENTRIES: usize = 128;
const MAX_AUDIO_CACHE_BYTES: usize = 32 * 1024 * 1024;

struct ReverseAudioCache {
    map: FxHashMap<String, (Arc<Vec<f32>>, u32)>,
    order: VecDeque<String>,
    in_flight: Vec<String>,
    total_bytes: usize,
}

impl Default for ReverseAudioCache {
    fn default() -> Self {
        Self {
            map: FxHashMap::default(),
            order: VecDeque::new(),
            in_flight: Vec::new(),
            total_bytes: 0,
        }
    }
}

struct DecodeAudioJob {
    key: String,
    bytes: Vec<u8>,
}

static AUDIO_CACHE: OnceLock<Mutex<ReverseAudioCache>> = OnceLock::new();
static AUDIO_TX: OnceLock<mpsc::Sender<DecodeAudioJob>> = OnceLock::new();

fn audio_cache() -> &'static Mutex<ReverseAudioCache> {
    AUDIO_CACHE.get_or_init(|| Mutex::new(ReverseAudioCache::default()))
}

fn decode_ogg_bytes(bytes: Vec<u8>) -> Option<(Vec<f32>, u32)> {
    use symphonia::core::{
        audio::SampleBuffer, codecs::DecoderOptions, formats::FormatOptions, io::MediaSourceStream,
        meta::MetadataOptions, probe::Hint,
    };
    let cursor = Cursor::new(bytes);
    let stream = MediaSourceStream::new(Box::new(cursor), Default::default());
    let mut hint = Hint::new();
    hint.with_extension("ogg");
    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            stream,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .ok()?;
    let mut format = probed.format;
    let track = format.default_track()?;
    let id = track.id;
    let rate = track.codec_params.sample_rate?;
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .ok()?;
    let mut mono_samples = Vec::new();
    while let Ok(packet) = format.next_packet() {
        if packet.track_id() != id {
            continue;
        }
        let Ok(buffer) = decoder.decode(&packet) else {
            continue;
        };
        let spec = *buffer.spec();
        let frame_channels = spec.channels.count().max(1);
        let mut decoded = SampleBuffer::<f32>::new(buffer.capacity() as u64, spec);
        decoded.copy_interleaved_ref(buffer);
        let raw_samples = decoded.samples();
        if frame_channels == 1 {
            mono_samples.extend_from_slice(raw_samples);
        } else {
            for frame in raw_samples.chunks(frame_channels) {
                let sum: f32 = frame.iter().sum();
                mono_samples.push(sum / frame_channels as f32);
            }
        }
    }
    if mono_samples.is_empty() {
        None
    } else {
        mono_samples.reverse();
        Some((mono_samples, rate))
    }
}

fn audio_worker_tx() -> &'static mpsc::Sender<DecodeAudioJob> {
    AUDIO_TX.get_or_init(|| {
        let (tx, rx) = mpsc::channel::<DecodeAudioJob>();
        let _ = std::thread::Builder::new()
            .name("rreturn-reverse-audio".to_owned())
            .spawn(move || {
                while let Ok(job) = rx.recv() {
                    let decoded = decode_ogg_bytes(job.bytes);
                    if let Ok(mut cache) = audio_cache().lock() {
                        cache.in_flight.retain(|k| k != &job.key);
                        if let Some((samples, rate)) = decoded {
                            let new_bytes = samples.len().saturating_mul(4);
                            if let Some((old_samples, _)) = cache.map.remove(&job.key) {
                                cache.total_bytes = cache
                                    .total_bytes
                                    .saturating_sub(old_samples.len().saturating_mul(4));
                            } else {
                                cache.order.push_back(job.key.clone());
                            }
                            cache.total_bytes = cache.total_bytes.saturating_add(new_bytes);
                            cache.map.insert(job.key, (Arc::new(samples), rate));
                            while cache.order.len() > MAX_AUDIO_CACHE_ENTRIES
                                || cache.total_bytes > MAX_AUDIO_CACHE_BYTES
                            {
                                if let Some(oldest) = cache.order.pop_front() {
                                    if let Some((evicted, _)) = cache.map.remove(&oldest) {
                                        cache.total_bytes = cache
                                            .total_bytes
                                            .saturating_sub(evicted.len().saturating_mul(4));
                                    }
                                } else {
                                    break;
                                }
                            }
                        }
                    }
                }
            });
        tx
    })
}

pub fn prewarm_reverse_sound(game: &Game<'_>, identifier: &str) {
    let mut rng = LegacyRandom::new(42);
    let norm = if identifier.contains(':') {
        identifier.to_owned()
    } else {
        format!("minecraft:{identifier}")
    };
    if norm.starts_with("minecraft:music.") || norm.starts_with("minecraft:music_disc.") {
        return;
    }
    let Some(sound_def) = game.audio.engine.registry.sound(&norm, &mut rng) else {
        return;
    };
    let (namespace, sound_path) = sound_def.path();
    let cache_key = format!("{namespace}:{sound_path}");
    let needs_decode = audio_cache()
        .lock()
        .map(|mut c| {
            if c.map.contains_key(&cache_key) || c.in_flight.contains(&cache_key) {
                false
            } else {
                c.in_flight.push(cache_key.clone());
                true
            }
        })
        .unwrap_or(false);
    if !needs_decode {
        return;
    }
    let asset_path = format!("assets/{namespace}/{sound_path}");
    if let Ok(bytes) = game.pack.source.read(&asset_path) {
        let _ = audio_worker_tx().send(DecodeAudioJob {
            key: cache_key,
            bytes,
        });
    } else if let Ok(mut c) = audio_cache().lock() {
        c.in_flight.retain(|k| k != &cache_key);
    }
}

pub fn play_scrub_sounds(
    game: &Game<'_>,
    sounds_to_play: Vec<(SoundRecord, bool)>,
    to_tick: i64,
    jog_accel: f64,
) {
    let mut rng = LegacyRandom::new(42);
    let speed_factor = (0.85 + (jog_accel - 1.0) * 0.04).clamp(0.75, 1.8) as f32;
    for (idx, (s, backwards)) in sounds_to_play.into_iter().take(4).enumerate() {
        let at = if s.relative { None } else { Some(s.position) };
        let norm = if s.identifier.contains(':') {
            s.identifier.clone()
        } else {
            format!("minecraft:{}", s.identifier)
        };
        if norm.starts_with("minecraft:music.") || norm.starts_with("minecraft:music_disc.") {
            continue;
        }
        if backwards {
            let mut played = false;
            if let Some(sound_def) = game.audio.engine.registry.sound(&norm, &mut rng) {
                let (namespace, sound_path) = sound_def.path();
                let cache_key = format!("{namespace}:{sound_path}");
                let cached = audio_cache()
                    .lock()
                    .ok()
                    .and_then(|c| c.map.get(&cache_key).cloned());
                if let Some((samples, rate)) = cached {
                    let scaled_rate = ((rate as f32) * speed_factor).clamp(8000.0, 96000.0) as u32;
                    let source_id = ((to_tick as u128) << 32) ^ (idx as u128) ^ 0x52524554;
                    if game
                        .audio
                        .push_voice("rreturn", source_id, &samples, scaled_rate, at)
                    {
                        played = true;
                    }
                } else {
                    prewarm_reverse_sound(game, &norm);
                }
            }
            if !played {
                let mut sound = if s.relative {
                    mod_api::SoundPlay::ui(&norm)
                } else {
                    mod_api::SoundPlay::at(
                        &norm,
                        mod_api::SoundSource::Records,
                        s.position,
                    )
                };
                sound.pitch = (0.85 * speed_factor).clamp(0.5, 2.0);
                sound.volume = 0.9;
                game.play_local_sound(&sound, 0);
            }
        } else {
            let mut sound = if s.relative {
                mod_api::SoundPlay::ui(&norm)
            } else {
                mod_api::SoundPlay::at(&norm, mod_api::SoundSource::Records, s.position)
            };
            sound.pitch = speed_factor.clamp(0.5, 2.0);
            sound.volume = 0.9;
            game.play_local_sound(&sound, 0);
        }
    }
}

pub fn clear_reverse_audio_cache() {
    if let Ok(mut c) = audio_cache().lock() {
        c.map.clear();
        c.order.clear();
        c.in_flight.clear();
        c.total_bytes = 0;
    }
}