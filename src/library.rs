//! Importing, replacing and removing sound files.

use std::path::{Path, PathBuf};

use crate::audio::decode;
use crate::config::{Board, PlayMode};
use crate::paths::Paths;

fn sanitize(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| if c.is_alphanumeric() || matches!(c, '.' | '-' | '_' | ' ') { c } else { '_' })
        .collect();
    let s = s.trim().trim_matches('.').to_string();
    if s.is_empty() { "sound".into() } else { s.chars().take(80).collect() }
}

/// Validate `src` and (optionally) copy it into the library. Returns the path to store.
fn bring_in(paths: &Paths, src: &Path, id: u64, copy: bool) -> Result<(PathBuf, Option<f64>), String> {
    if !src.is_file() {
        return Err("file not found".into());
    }
    if !decode::is_supported_extension(src) {
        return Err("unsupported file type (use MP3, WAV, OGG, FLAC or M4A)".into());
    }
    let duration = decode::probe(src)?.duration_secs;
    if !copy {
        return Ok((src.to_path_buf(), duration));
    }
    let dir = paths.sounds_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("could not create {}: {e}", dir.display()))?;
    let fname = src.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let dest = dir.join(format!("{id}-{}", sanitize(&fname)));
    std::fs::copy(src, &dest).map_err(|e| format!("could not copy into the library: {e}"))?;
    Ok((paths.store(&dest), duration))
}

fn display_name(src: &Path) -> String {
    src.file_stem().map(|s| s.to_string_lossy().replace(['_'], " ")).unwrap_or_else(|| "Sound".into())
}

pub struct ImportReport {
    pub added: Vec<u64>,
    pub errors: Vec<String>,
}

pub fn import(paths: &Paths, board: &mut Board, files: &[PathBuf], copy: bool, mode: PlayMode) -> ImportReport {
    let mut report = ImportReport { added: Vec::new(), errors: Vec::new() };
    for f in files {
        let name = f.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let id = board.next_id;
        match bring_in(paths, f, id, copy) {
            Ok((stored, duration)) => {
                let id = board.add(display_name(f), stored, mode);
                if let Some(s) = board.get_mut(id) {
                    s.duration = duration;
                }
                report.added.push(id);
                log::info!("imported {} as sound {id}", f.display());
            }
            Err(e) => {
                log::warn!("import of {} failed: {e}", f.display());
                report.errors.push(format!("{name}: {e}"));
            }
        }
    }
    report
}

/// Remove the library copy of a sound file (only files FreeMic copied itself).
fn delete_library_copy(paths: &Paths, stored: &Path) {
    if stored.is_relative() {
        let p = paths.resolve(stored);
        if p.starts_with(paths.sounds_dir()) {
            let _ = std::fs::remove_file(p);
        }
    }
}

pub fn replace(paths: &Paths, board: &mut Board, id: u64, file: &Path, copy: bool) -> Result<(), String> {
    let (stored, duration) = bring_in(paths, file, board.next_id, copy)?;
    board.next_id += 1;
    let sound = board.get_mut(id).ok_or("sound no longer exists")?;
    sound.duration = duration;
    let old = std::mem::replace(&mut sound.file, stored);
    delete_library_copy(paths, &old);
    Ok(())
}

pub fn remove(paths: &Paths, board: &mut Board, id: u64) {
    if let Some(s) = board.remove(id) {
        delete_library_copy(paths, &s.file);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn import_copy_replace_remove() {
        let root = std::env::temp_dir().join(format!("freemic-lib-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let paths = Paths { portable: true, config_dir: root.clone(), data_dir: root.clone() };
        let fx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
        let mut board = Board::default();
        let rep = import(
            &paths,
            &mut board,
            &[fx.join("stereo_44k.mp3"), fx.join("not_audio.mp3"), fx.join("readme.txt")],
            true,
            PlayMode::Once,
        );
        assert_eq!(rep.added.len(), 1);
        assert_eq!(rep.errors.len(), 2, "{:?}", rep.errors);
        let s = board.get(rep.added[0]).unwrap().clone();
        assert!(s.file.is_relative(), "library copy stored relative: {:?}", s.file);
        assert!(paths.resolve(&s.file).is_file());
        assert_eq!(s.name, "stereo 44k");

        replace(&paths, &mut board, s.id, &fx.join("stereo_48k.ogg"), true).unwrap();
        let s2 = board.get(s.id).unwrap().clone();
        assert!(!paths.resolve(&s.file).exists(), "old copy deleted");
        assert!(paths.resolve(&s2.file).is_file());

        remove(&paths, &mut board, s.id);
        assert!(board.sounds.is_empty());
        assert!(!paths.resolve(&s2.file).exists());
        let _ = std::fs::remove_dir_all(&root);
    }
}
