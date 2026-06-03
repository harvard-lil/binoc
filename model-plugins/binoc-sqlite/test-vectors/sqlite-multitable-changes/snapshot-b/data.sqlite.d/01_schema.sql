CREATE TABLE users (
    id INTEGER PRIMARY KEY,
    name TEXT
);

CREATE TABLE products (
    id INTEGER PRIMARY KEY,
    name TEXT
);

CREATE TABLE orders (
    id INTEGER PRIMARY KEY,
    user_id INTEGER,
    total REAL
);
