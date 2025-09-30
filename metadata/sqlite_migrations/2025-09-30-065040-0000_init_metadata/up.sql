-- Your SQL goes here
create table projects
(
    id                   text     default (lower(hex(randomblob(16)))) not null
        primary key,
    name                 text                                   not null,
    description          text,
    created_at           text     default (datetime('now'))      not null,
    updated_at           text     default (datetime('now'))      not null,
    slug                 text                                   not null
        unique,
    settings             text     default '{"enabled_chat_tracing": true}',
    is_default           integer  default 0                      not null,
    archived_at          text,
    allowed_user_ids     text,
    private_model_prices text
);

