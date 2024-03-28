-- Add migration script here
CREATE TABLE `coin_states` (
    `coin_id` BLOB NOT NULL PRIMARY KEY,
    `parent_coin_info` BLOB NOT NULL,
    `puzzle_hash` BLOB NOT NULL,
    `amount` BIGINT UNSIGNED NOT NULL,
    `created_height` INT UNSIGNED,
    `spent_height` INT UNSIGNED,
    `asset_id` BLOB NOT NULL
);

CREATE TABLE `transactions` (
    `transaction_id` BLOB NOT NULL PRIMARY KEY,
    `aggregated_signature` BLOB NOT NULL
);

CREATE TABLE `coin_spends` (
    `transaction_id` BLOB NOT NULL,
    `coin_id` BLOB NOT NULL,
    `parent_coin_id` BLOB NOT NULL,
    `puzzle_hash` BLOB NOT NULL,
    `amount` BIGINT UNSIGNED NOT NULL,
    `puzzle_reveal` BLOB NOT NULL,
    `solution` BLOB NOT NULL,
    PRIMARY KEY (`transaction_id`, `coin_id`),
    FOREIGN KEY(`transaction_id`)
        REFERENCES `transactions`(`transaction_id`)
        ON DELETE CASCADE
);
