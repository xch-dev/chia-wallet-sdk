-- Add migration script here
CREATE TABLE `hardened_keys` (
    `index` INT UNSIGNED NOT NULL PRIMARY KEY,
    `public_key` BLOB NOT NULL,
    `p2_puzzle_hash` BLOB NOT NULL
);

CREATE TABLE `unhardened_keys` (
    `index` INT UNSIGNED NOT NULL PRIMARY KEY,
    `public_key` BLOB NOT NULL,
    `p2_puzzle_hash` BLOB NOT NULL
);