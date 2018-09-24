extern crate diesel;
extern crate r2d2;
extern crate r2d2_diesel;
#[macro_use]
extern crate slog;
extern crate slog_async;
extern crate slog_term;

use std::error::Error;
use std::fmt::Display;

#[derive(Debug)]
pub enum MyError {
    StringErr(String),
}

impl Display for MyError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        match *self {
            MyError::StringErr(ref err) => write!(formatter, "StringError: {:?}", err),
        }
    }
}

impl Error for MyError {}

type HandleResult<T> = Result<T, ()>;

pub trait WithHandle<'a> {
    type Handle;
    type Config;
    // withHandle :: Config -> (Handle -> IO a) -> IO a
    fn with_handle<T, F>(config: &'a Self::Config, f: F) -> HandleResult<T>
    where
        F: FnOnce(&Self::Handle) -> HandleResult<T>;
}

pub trait WithHandle2<'a> {
    type Handle;
    type Handle2;
    type Config;

    fn with_handle2<T, F>(
        config: &'a Self::Config,
        handle_2: &'a Self::Handle2,
        f: F,
    ) -> HandleResult<T>
    where
        F: FnOnce(&Self::Handle) -> HandleResult<T>;
}

pub trait WithHandle3<'a> {
    type Handle;
    type Handle2;
    type Handle3;
    type Config;

    fn with_handle3<T, F>(
        config: &'a Self::Config,
        handle2: &'a Self::Handle2,
        handle3: &'a Self::Handle3,
        f: F,
    ) -> HandleResult<T>
    where
        F: FnOnce(&Self::Handle) -> HandleResult<T>;
}

pub trait WithHandle4<'a> {
    type Handle;
    type Handle2;
    type Handle3;
    type Handle4;
    type Config;

    fn with_handle4<T, F>(
        config: &'a Self::Config,
        handle2: &'a Self::Handle2,
        handle3: &'a Self::Handle3,
        handle4: &'a Self::Handle4,
        f: F,
    ) -> HandleResult<T>
    where
        F: FnOnce(&Self::Handle) -> HandleResult<T>;
}

pub trait WithHandle5<'a> {
    type Handle;
    type Handle2;
    type Handle3;
    type Handle4;
    type Handle5;
    type Config;

    fn with_handle5<T, F>(
        config: &'a Self::Config,
        handle2: &'a Self::Handle2,
        handle3: &'a Self::Handle3,
        handle4: &'a Self::Handle4,
        handle5: &'a Self::Handle5,
        f: F,
    ) -> HandleResult<T>
    where
        F: FnOnce(&Self::Handle) -> HandleResult<T>;
}

mod logger {
    use super::{HandleResult, WithHandle};
    use slog::{self, Drain};
    use slog_async;
    use slog_term;
    use std::error::Error;
    use std::fs::OpenOptions;
    use std::path::Path;
    use std::sync::{Arc, Mutex};

    #[derive(Clone, PartialEq, PartialOrd)]
    pub enum Verbosity {
        Debug,
        Info,
        Warning,
        Error,
    }

    pub enum LoggerType<'a> {
        File(&'a Path),
        Term,
        Sentry(&'a str),
    }

    #[derive(Clone)]
    pub struct Config<'a> {
        // TODO: Add a new fn instead of using pub.
        pub verbosity: Verbosity,
        pub loggers: Vec<&'a LoggerType<'a>>,
    }

    pub struct Handle<'a, 'b: 'a> {
        config: &'a Config<'b>,
        logger: slog::Logger,
    }

    impl<'a> WithHandle<'a> for Handle<'a, 'a> {
        type Handle = Self;
        type Config = Config<'a>;

        fn with_handle<T, F>(config: &'a Self::Config, f: F) -> HandleResult<T>
        where
            F: FnOnce(&Self::Handle) -> HandleResult<T>,
        {
            let Config { loggers, .. } = config;

            let drains = if let Some((head, tail)) = loggers.split_first() {
                let root_drain = match head {
                    LoggerType::File(path) => Some(init_file_logger(path)),
                    LoggerType::Term => Some(init_term_logger()),
                    LoggerType::Sentry(_) => None,
                };

                let fuses = tail
                    .iter()
                    .map(|logger_type| match logger_type {
                        LoggerType::File(path) => Some(init_file_logger(path)),
                        LoggerType::Term => Some(init_term_logger()),
                        LoggerType::Sentry(_) => None,
                    })
                    .filter_map(|fuse_option| fuse_option)
                    .collect::<Vec<slog::Fuse<slog_async::Async>>>();

                let root_drain = root_drain.unwrap();

                fuses.into_iter().fold(
                    Box::new(root_drain) as Box<dyn Drain<Err = _, Ok = _> + Send + Sync>,
                    |prev, curr| {
                        Box::new(slog::Duplicate::new(prev, curr).fuse())
                            as Box<dyn Drain<Err = _, Ok = _> + Send + Sync>
                    },
                )
            } else {
                Box::new(slog::Discard)
            };

            let logger_root = slog::Logger::root(
                Arc::new(
                    Mutex::new(drains)
                        .map_err::<_, slog::Never>(|_| panic!("A logging error occurred")),
                ),
                o!(),
            );

            let handle = Handle {
                config,
                logger: logger_root,
            };
            f(&handle)
        }
    }

    fn init_term_logger() -> slog::Fuse<slog_async::Async> {
        let decorator = slog_term::TermDecorator::new().build();
        let drain = slog_term::FullFormat::new(decorator).build().fuse();
        slog_async::Async::new(drain).build().fuse()
    }

    fn init_file_logger(path: &Path) -> slog::Fuse<slog_async::Async> {
        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(path)
            .unwrap();

        let decorator = slog_term::PlainDecorator::new(file);
        let drain = slog_term::FullFormat::new(decorator).build().fuse();
        slog_async::Async::new(drain).build().fuse()
    }

    struct Log<'a> {
        verbosity: &'a Verbosity,
        string: Option<&'a str>,
        error: Option<Box<Error>>,
    }

    fn log(handle: &Handle, log: Log) -> Result<(), ()> {
        let Log {
            verbosity,
            string,
            error,
        } = log;

        let has_enough_verbosity = *verbosity >= handle.config.verbosity;

        match verbosity {
            Verbosity::Debug if has_enough_verbosity => {
                debug!(handle.logger, "{}", string.unwrap_or(""))
            }
            Verbosity::Debug => {}
            Verbosity::Info if has_enough_verbosity => {
                info!(handle.logger, "{}", string.unwrap_or(""))
            }
            Verbosity::Info => {}
            Verbosity::Warning if has_enough_verbosity => {
                warn!(handle.logger, "{}", string.unwrap_or(""))
            }
            Verbosity::Warning => {}
            Verbosity::Error => error!(handle.logger, "{}", error.unwrap()),
        }

        Ok(())
    }

    pub fn debug(handle: &Handle, string: &str) -> Result<(), ()> {
        log(
            handle,
            Log {
                verbosity: &Verbosity::Debug,
                string: Some(string),
                error: None,
            },
        )
    }

    pub fn info(handle: &Handle, string: &str) -> Result<(), ()> {
        log(
            handle,
            Log {
                verbosity: &Verbosity::Info,
                string: Some(string),
                error: None,
            },
        )
    }

    pub fn warning(handle: &Handle, string: &str) -> Result<(), ()> {
        log(
            handle,
            Log {
                verbosity: &Verbosity::Warning,
                string: Some(string),
                error: None,
            },
        )
    }

    pub fn error<E: 'static>(handle: &Handle, error: E) -> Result<(), ()>
    where
        E: Error,
    {
        log(
            handle,
            Log {
                verbosity: &Verbosity::Error,
                string: None,
                error: Some(Box::new(error)),
            },
        )
    }
}
mod database {
    use super::logger;
    use super::{HandleResult, WithHandle2};
    use diesel::pg::PgConnection;
    use r2d2;
    use r2d2_diesel::ConnectionManager;
    use MyError;

    #[derive(Clone)]
    pub struct Config<'a> {
        connection_string: &'a str,
    }

    impl<'a> Config<'a> {
        pub fn new(connection_string: &'a str) -> Self {
            Config { connection_string }
        }
    }

    pub struct Handle<'a, 'b: 'a> {
        pool: Pool,
        logger_handle: &'a logger::Handle<'b, 'b>,
    }

    impl<'a> WithHandle2<'a> for Handle<'a, 'a> {
        type Handle = Self;
        type Handle2 = logger::Handle<'a, 'a>;
        type Config = Config<'a>;

        fn with_handle2<T, F>(
            config: &'a Self::Config,
            handle2: &'a Self::Handle2,
            f: F,
        ) -> HandleResult<T>
        where
            F: FnOnce(&Self::Handle) -> HandleResult<T>,
        {
            let pool = init_pool(&config.connection_string);
            let handle = Handle {
                pool,
                logger_handle: handle2,
            };

            f(&handle)
        }
    }

    pub type Pool = r2d2::Pool<ConnectionManager<PgConnection>>;

    pub fn init_pool(database_url: &str) -> Pool {
        let manager = ConnectionManager::<PgConnection>::new(database_url);
        r2d2::Pool::builder()
            .build(manager)
            .expect("Failed to create database pool.")
    }

    #[derive(Debug)]
    pub struct User {
        pub id: String,
        pub name: String,
    }

    pub fn create_user(handle: &Handle, user: User) -> Result<Vec<User>, MyError> {
        let s = format!("Failed to create user: {:?}", user);

        let _ = logger::info(handle.logger_handle, &format!("Creating user: {:?}", user));

        let _created_user = handle.pool.get().map(|_db_conn| vec![user]).unwrap();

        Err(MyError::StringErr(s))
    }
}

#[derive(Clone)]
struct Config<'a> {
    logger_config: logger::Config<'a>,
    database_config: database::Config<'a>,
}

struct Handle<'a> {
    logger_handle: &'a logger::Handle<'a, 'a>,
    database_handle: &'a database::Handle<'a, 'a>,
}

impl<'a> WithHandle3<'a> for Handle<'a> {
    type Config = Config<'a>;
    type Handle = Handle<'a>;
    type Handle2 = logger::Handle<'a, 'a>;
    type Handle3 = database::Handle<'a, 'a>;

    fn with_handle3<T, F>(
        _config: &'a Self::Config,
        logger: &'a Self::Handle2,
        database: &'a Self::Handle3,
        f: F,
    ) -> HandleResult<T>
    where
        F: FnOnce(&Self::Handle) -> HandleResult<T>,
    {
        f(&Handle {
            logger_handle: logger,
            database_handle: database,
        })
    }
}

fn run(handle: &Handle) -> Result<(), ()> {
    use database::{create_user, User};

    let user = User {
        id: "1".to_owned(),
        name: "Sherlock".to_owned(),
    };

    let _created_user = create_user(handle.database_handle, user)
        .map_err(|err| {
            let _ = logger::error(handle.logger_handle, err);
        })
        .map(|created_user| {
            let _ = logger::info(
                handle.logger_handle,
                &format!("Created users {:?}", created_user),
            );
            created_user
        });

    let _ = logger::debug(handle.logger_handle, "Running...");
    let _ = logger::warning(handle.logger_handle, "Warning!");

    Ok(())
}

fn main() -> Result<(), ()> {
    let file_logger = logger::LoggerType::File(std::path::Path::new("./logs/x.log"));
    let config = Config {
        logger_config: logger::Config {
            verbosity: logger::Verbosity::Info,
            loggers: vec![&file_logger, &logger::LoggerType::Term],
        },
        database_config: database::Config::new("postgres://postgres:password@localhost/postgres"),
    };

    logger::Handle::with_handle(&config.logger_config, |log_handle| {
        database::Handle::with_handle2(&config.database_config, log_handle, |db_handle| {
            Handle::with_handle3(&config, log_handle, db_handle, |app_handle| run(app_handle))
        })
    })
}
