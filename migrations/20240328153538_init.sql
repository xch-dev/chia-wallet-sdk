-- Add migration script here
CREATE TABLE `standard_coin_states` (
    `coin_id` BLOB NOT NULL PRIMARY KEY,
    `parent_coin_info` BLOB NOT NULL,
    `puzzle_hash` BLOB NOT NULL,
    `amount` BIGINT UNSIGNED NOT NULL,
    `created_height` INT UNSIGNED,
    `spent_height` INT UNSIGNED
);

CREATE TABLE `cat_coin_states` (
    `coin_id` BLOB NOT NULL PRIMARY KEY,
    `parent_coin_info` BLOB NOT NULL,
    `puzzle_hash` BLOB NOT NULL,
    `amount` BIGINT UNSIGNED NOT NULL,
    `created_height` INT UNSIGNED,
    `spent_height` INT UNSIGNED,
    `asset_id` BLOB NOT NULL
);
