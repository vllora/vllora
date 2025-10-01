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

create table threads
(
    id                   text     default (lower(hex(randomblob(16)))) not null
        primary key,
    user_id              text,
    model_name           text,
    created_at           text     default (datetime('now'))             not null,
    tenant_id            text,
    project_id           text,
    is_public            integer  default 0                             not null,
    description          text,
    keywords             text     default '[]'                          not null
);

create table messages
(
    id                   text     default (lower(hex(randomblob(16)))) not null
        primary key,
    model_name           text,
    type                 text,
    thread_id            text,
    user_id              text,
    content_type         text,
    content              text,
    content_array        text     default '[]'                          not null,
    tool_call_id         text,
    tool_calls           text,
    tenant_id            text,
    project_id           text,
    created_at           text     default (datetime('now'))             not null,
    foreign key (thread_id) references threads(id)
);

