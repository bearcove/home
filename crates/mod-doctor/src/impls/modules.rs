use super::DoctorError;

pub async fn check_modules() -> Vec<DoctorError> {
    struct ModuleToLoad {
        name: &'static str,
        load: fn(),
    }

    macro_rules! module_list {
            ($($name:ident),*) => {
                vec![
                    $(
                        ModuleToLoad {
                            name: stringify!($name),
                            load: || {
                                $name::load();
                            },
                        },
                    )*
                ]
            }
        }

    let modules_to_load = module_list!(
        github,
        clap,
        lightningcss,
        cub,
        revision,
        doctor,
        errhandling,
        svg,
        mom,
        template,
        momclient,
        search,
        highlight,
        api,
        image,
        config,
        websock,
        compress,
        htmlrewrite,
        math,
        tracingsub,
        term,
        objectstore,
        reddit,
        cdn,
        markdown,
        libhttpclient,
        webpage,
        fs,
        media,
        patreon
    );

    struct ModuleHandle {
        name: &'static str,
        handle: std::thread::JoinHandle<()>,
    }

    let handles: Vec<ModuleHandle> = modules_to_load
        .into_iter()
        .map(|module| {
            let name = module.name;
            let handle = std::thread::spawn(move || {
                (module.load)();
            });
            ModuleHandle { name, handle }
        })
        .collect();

    let mut errors = Vec::new();

    for module_handle in handles {
        if let Err(e) = module_handle.handle.join() {
            errors.push(DoctorError::ModuleLoadFailed {
                name: module_handle.name.to_string(),
                error: format!("{:?}", e),
            });
        }
    }

    errors
}
