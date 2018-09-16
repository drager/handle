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

    fn with_handle_2<T, F>(
        config: &'a Self::Config,
        handle_2: &'a Self::Handle2,
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

    #[derive(Clone)]
    pub enum Verbosity {
        Debug,
        Info,
        Warning,
        Error,
    }

    #[derive(Clone)]
    pub struct Config<'a> {
        // TODO: Add a new fn instead of using pub.
        pub path: Option<&'a Path>,
        pub verbosity: Verbosity,
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
            let drain = if let Some(path) = config.path {
                let file = OpenOptions::new()
                    .create(true)
                    .write(true)
                    .truncate(true)
                    .open(path)
                    .unwrap();

                let decorator = slog_term::PlainDecorator::new(file);
                let drain = slog_term::FullFormat::new(decorator).build().fuse();
                let drain = slog_async::Async::new(drain).build().fuse();
                drain
            } else {
                let decorator = slog_term::TermDecorator::new().build();
                let drain = slog_term::FullFormat::new(decorator).build().fuse();
                let drain = slog_async::Async::new(drain).build().fuse();
                drain
            };

            let logger = slog::Logger::root(drain, o!());
            logger.new(o!());
            let handle = Handle { config, logger };
            let handle_before = f(&handle);
            handle_before
        }
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

        match verbosity {
            Verbosity::Debug => debug!(handle.logger, "{}", string.unwrap_or("")),
            Verbosity::Info => info!(handle.logger, "{}", string.unwrap_or("")),
            Verbosity::Warning => warn!(handle.logger, "{}", string.unwrap_or("")),
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

    // TODO: Add a new fn instead of using pub.
    #[derive(Clone)]
    pub struct Config {
        pub connection_string: String,
    }

    pub struct Handle<'a, 'b: 'a> {
        config: &'a Config,
        pool: Pool,
        logger_handle: &'a logger::Handle<'b, 'b>,
    }

    impl<'a> WithHandle2<'a> for Handle<'a, 'a> {
        type Handle = Self;
        type Handle2 = logger::Handle<'a, 'a>;
        type Config = Config;

        fn with_handle_2<T, F>(
            config: &'a Self::Config,
            handle_2: &'a Self::Handle2,
            f: F,
        ) -> HandleResult<T>
        where
            F: FnOnce(&Self::Handle) -> HandleResult<T>,
        {
            let pool = init_pool(&config.connection_string);
            let handle = Handle {
                config,
                pool,
                logger_handle: handle_2,
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
        let _created_user = handle.pool.get().map(|_db_conn| vec![user]).unwrap();

        Err(MyError::StringErr(s))
    }
}

#[derive(Clone)]
struct Config<'a> {
    logger_config: logger::Config<'a>,
    database_config: database::Config,
}

struct Handle<'a> {
    logger_handle: &'a logger::Handle<'a, 'a>,
    database_handle: &'a database::Handle<'a, 'a>,
}

// TODO: Should have a trait: WithHandle3, WithHandle4, WithHandle5.
fn with_handle<T, F>(
    _config: &Config,
    logger: &logger::Handle,
    database: &database::Handle,
    f: F,
) -> HandleResult<T>
where
    F: FnOnce(Handle) -> HandleResult<T>,
{
    f(Handle {
        logger_handle: logger,
        database_handle: database,
    })
}

fn run(handle: Handle) -> Result<(), ()> {
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
    let config = Config {
        logger_config: logger::Config {
            verbosity: logger::Verbosity::Debug,
            path: Some(std::path::Path::new("./logs/x.log")),
        },
        database_config: database::Config {
            connection_string: "postgres://postgres:password@localhost/postgres".to_owned(),
        },
    };

    logger::Handle::with_handle(&config.logger_config, |log_handle| {
        database::Handle::with_handle_2(&config.database_config, log_handle, |db_handle| {
            with_handle(&config, log_handle, db_handle, |app_handle| run(app_handle))
        })
    })
}
