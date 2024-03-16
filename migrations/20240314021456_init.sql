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

CREATE TABLE `hardened_cats` (
    `puzzle_hash` BLOB NOT NULL PRIMARY KEY,
    `asset_id` BLOB NOT NULL,
    `public_key` BLOB NOT NULL,
    `puzzle_hash` BLOB NOT NULL
);

CREATE TABLE `coin_states` (
    `coin_id` BLOB NOT NULL PRIMARY KEY,
    `parent_coin_info` BLOB NOT NULL,
    `puzzle_hash` BLOB NOT NULL,
    `amount` BIGINT UNSIGNED NOT NULL,
    `created_height` INT UNSIGNED,
    `spent_height` INT UNSIGNED
);