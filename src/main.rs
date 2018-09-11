#[macro_use]
extern crate diesel;
extern crate r2d2;
extern crate r2d2_diesel;
mod schema;

type HandleResult<T> = Result<T, ()>;

pub trait WithHandle {
    type Handle;
    type Config;
    // withHandle :: Config -> (Handle -> IO a) -> IO a
    fn with_handle<T, F>(config: Self::Config, f: F) -> HandleResult<T>
    where
        F: FnOnce(Self::Handle) -> HandleResult<T>;
}

mod logger {
    use super::{HandleResult, WithHandle};
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

    pub struct Handle<'a> {
        config: Config<'a>,
        logger: Box<FnOnce() -> ()>,
    }

    impl<'a> WithHandle for Handle<'a> {
        type Handle = Self;
        type Config = Config<'a>;

        fn with_handle<T, F>(config: Self::Config, f: F) -> HandleResult<T>
        where
            F: FnOnce(Self::Handle) -> HandleResult<T>,
        {
            let handle = Handle {
                config: config,
                logger: Box::new(|| println!("Logger!")),
            };
            let handle_before = f(handle);
            handle_before
        }
    }

    /*    pub fn log(handle: Handle) -> Result<(), ()> {*/
    //println!();
    /*}*/

}
mod database {
    use super::{HandleResult, WithHandle};
    use diesel::pg::PgConnection;
    use r2d2;
    use r2d2_diesel::ConnectionManager;

    // TODO: Add a new fn instead of using pub.
    #[derive(Clone)]
    pub struct Config {
        pub connection_string: String,
    }

    pub struct Handle {
        config: Config,
        pool: Pool,
    }

    impl WithHandle for Handle {
        type Handle = Self;
        type Config = Config;

        fn with_handle<T, F>(config: Self::Config, f: F) -> HandleResult<T>
        where
            F: FnOnce(Self::Handle) -> HandleResult<T>,
        {
            let pool = init_pool(&config.connection_string);
            let handle = Handle { config, pool };

            f(handle)
        }
    }

    pub type Pool = r2d2::Pool<ConnectionManager<PgConnection>>;

    pub fn init_pool(database_url: &str) -> Pool {
        let manager = ConnectionManager::<PgConnection>::new(database_url);
        r2d2::Pool::builder()
            .build(manager)
            .expect("Failed to create database pool.")
    }

    pub struct DbConnection(pub r2d2::PooledConnection<ConnectionManager<PgConnection>>);

    use schema::users;

    #[derive(QueryableByName)]
    #[table_name = "users"]
    pub struct User {
        pub id: String,
        pub name: String,
    }

    pub fn create_user(handle: Handle, user: User) -> Result<Vec<User>, ()> {
        use diesel::{self, RunQueryDsl};

        handle
            .pool
            .get()
            .map_err(|_| ())
            .and_then(|db_conn| {
                let users: Result<Vec<User>, _> = diesel::sql_query("SELECT * FROM users")
                    .load(&*db_conn)
                    .map_err(|err| println!("err {:?}", err));
                users
            })
            .map(|users| {
                println!("Created users!");
                users
            })
    }
}

#[derive(Clone)]
struct Config<'a> {
    logger_config: logger::Config<'a>,
    database_config: database::Config,
}

struct Handle<'a> {
    logger_handle: logger::Handle<'a>,
    database_handle: database::Handle,
}

fn with_handle<T, F>(
    _config: Config,
    logger: logger::Handle,
    database: database::Handle,
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

    let created_user = create_user(handle.database_handle, user);

    Ok(())
}

fn main() -> Result<(), ()> {
    let config = Config {
        logger_config: logger::Config {
            verbosity: logger::Verbosity::Debug,
            path: None,
        },
        database_config: database::Config {
            connection_string: "postgres://postgres:password@localhost/postgres".to_owned(),
        },
    };

    let cloned_config = config.clone();
    let Config {
        logger_config,
        database_config,
    } = config;

    logger::Handle::with_handle(logger_config, |log| {
        database::Handle::with_handle(database_config, |db| {
            with_handle(cloned_config, log, db, |app_handle| run(app_handle))
        })
    })
}
