//! Same owned staged inode, real byte changes through the existing import seam.
//! These are integrity fixtures, not fabricated writes or hostile-UID guarantees.
use super::*;
use std::{io::Seek, os::unix::fs::FileExt};

fn staged_change_refuses(change: &str, late: bool) {
    let mut f = FaultFixture::new();
    let root = f.root.clone();
    let orphan = f.orphan();
    let len = f.bytes.len() as u64;
    let mut expected = f.bytes.clone();
    match change {
        "modify" => expected[0] ^= 1,
        "truncate" => { expected.pop().unwrap(); },
        "append" => expected.extend_from_slice(b"unexpected tail"),
        _ => unreachable!(),
    }
    let mut held_identity = None;
    let mut changed_identity = None;
    let mut staged_manifest = None;
    let mut injected = false;
    let result = f.store.import_inner(OsStr::new("staged-integrity"), &f.reader, &AtomicBool::new(false), |stage, file| {
        if stage == ImportStage::PayloadStaging {
            let m = file.metadata().unwrap();
            held_identity = Some((m.dev(), m.ino()));
        }
        let inject = if late { stage == ImportStage::ManifestReady }
            else { stage == ImportStage::ChunkWritten && file.stream_position().unwrap() == len };
        if inject {
            assert!(!injected);
            let mut renamed;
            let payload = if late {
                assert!(!root.join(PAYLOAD_PENDING).exists());
                renamed = fs::OpenOptions::new().read(true).write(true).open(root.join(&orphan)).unwrap();
                &mut renamed
            } else { &mut *file };
            let m = payload.metadata().unwrap();
            assert_eq!(Some((m.dev(), m.ino())), held_identity, "must mutate the held staged inode, never replace it");
            assert_eq!(m.nlink(), 1);
            assert_eq!(m.len(), len);
            match change {
                "modify" => payload.write_all_at(&expected[..1], 0).unwrap(),
                "truncate" => payload.set_len(len - 1).unwrap(),
                "append" => payload.write_all_at(b"unexpected tail", len).unwrap(),
                _ => unreachable!(),
            }
            payload.sync_all().unwrap();
            let m = payload.metadata().unwrap();
            changed_identity = Some((m.dev(), m.ino()));
            injected = true;
        }
        if stage == ImportStage::ManifestReady {
            file.rewind().unwrap();
            let mut bytes = Vec::new();
            file.read_to_end(&mut bytes).unwrap();
            let candidate = Manifest::decode(&bytes).unwrap();
            assert_eq!(candidate.items.len(), f.kept.len() + 1);
            let item = candidate.items.last().unwrap();
            assert_eq!(item.bytes, len);
            assert_eq!(item.sha256, <[u8; 32]>::from(Sha256::digest(&f.bytes)));
            staged_manifest = Some(bytes);
        }
        Ok(())
    });
    assert!(injected);
    assert_eq!(changed_identity, held_identity);
    let payload = root.join(&orphan);
    let m = fs::metadata(&payload).unwrap();
    assert_eq!(Some((m.dev(), m.ino())), held_identity);
    assert_eq!(fs::read(&payload).unwrap(), expected, "check actual invalid bytes before checking the result");
    assert_ne!(expected, f.bytes);
    println!("STAGED_INTEGRITY change={change} late={late} result={result:?} committed_manifest_changed={}", fs::read(root.join(MANIFEST)).unwrap() != f.manifest);
    match result {
        Err(Failure::Refused(reason)) => assert!(reason.contains(if change == "modify" { "digest" } else { "length" }), "wrong refusal: {reason}"),
        other => panic!("invalid staged {change}, late={late}, reported as {other:?}"),
    }
    f.check(vec![(orphan, expected), (MANIFEST_PENDING.into(), staged_manifest.unwrap())]);
}

#[test]
fn staged_integrity_modify_after_last_chunk_refuses() { staged_change_refuses("modify", false); }
#[test]
fn staged_integrity_truncate_after_last_chunk_refuses() { staged_change_refuses("truncate", false); }
#[test]
fn staged_integrity_append_after_last_chunk_refuses() { staged_change_refuses("append", false); }
#[test]
fn staged_integrity_modify_after_rename_manifest_ready_refuses() { staged_change_refuses("modify", true); }
#[test]
fn staged_integrity_truncate_after_rename_manifest_ready_refuses() { staged_change_refuses("truncate", true); }
#[test]
fn staged_integrity_append_after_rename_manifest_ready_refuses() { staged_change_refuses("append", true); }
