use crate::{Error, Runtime, fs};

#[test]
fn file_requests_use_owned_inputs_enforce_read_limits_and_report_errors() {
    let path = std::env::temp_dir().join(format!("vthread-file-{}", std::process::id()));
    let work_path = path.clone();
    Runtime::new()
        .unwrap()
        .scope(|scope| {
            scope
                .spawn("files", move || {
                    fs::write(&work_path, b"data".to_vec())?;
                    assert_eq!(fs::read(&work_path, 4)?, b"data");
                    assert!(matches!(
                        fs::read(&work_path, 3),
                        Err(Error::LimitExceeded { limit: 3, .. })
                    ));
                    assert_eq!(fs::metadata(&work_path)?.len(), 4);
                    assert!(matches!(
                        fs::read(work_path.with_extension("missing"), 8),
                        Err(Error::Io(_))
                    ));
                    Ok::<_, Error>(())
                })?
                .join()??;
            Ok(())
        })
        .unwrap();
    std::fs::remove_file(path).unwrap();
}
#[test]
fn a_small_file_does_not_reserve_the_callers_large_limit() {
    let runtime = crate::Runtime::new().unwrap();
    for limit in [16 * 1024 * 1024, usize::MAX] {
        let data = runtime
            .scope(|scope| {
                scope
                    .spawn("small file", move || super::read("Cargo.toml", limit))?
                    .join()?
            })
            .unwrap();
        assert!(!data.is_empty());
        assert!(
            data.capacity() <= 8192,
            "tiny read eagerly reserved {} bytes",
            data.capacity()
        );
    }
}
