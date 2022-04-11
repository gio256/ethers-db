use akula::models::H256;

// Copied from akula
#[macro_export]
macro_rules! decl_table {
    ($name:ident => $key:ty => $value:ty => $seek_key:ty) => {
        #[derive(Clone, Copy, Debug, Default)]
        pub struct $name;

        impl akula::kv::traits::Table for $name {
            type Key = $key;
            type SeekKey = $seek_key;
            type Value = $value;

            fn db_name(&self) -> string::String<bytes::Bytes> {
                unsafe {
                    string::String::from_utf8_unchecked(bytes::Bytes::from_static(
                        Self::const_db_name().as_bytes(),
                    ))
                }
            }
        }

        impl $name {
            pub const fn const_db_name() -> &'static str {
                stringify!($name)
            }

            pub const fn erased(self) -> akula::kv::tables::ErasedTable<Self> {
                akula::kv::tables::ErasedTable(self)
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{}", Self::const_db_name())
            }
        }
    };
    ($name:ident => $key:ty => $value:ty) => {
        decl_table!($name => $key => $value => $key);
    };
}

decl_table!(LastBlock => Vec<u8> => H256);
decl_table!(LastHeader => Vec<u8> => H256);
decl_table!(PlainState => akula::models::Address => Vec<u8>);
