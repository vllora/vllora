CREATE INDEX IF NOT EXISTS idx_traces_title_first ON traces(project_id, thread_id, start_time_us) WHERE json_extract(attribute,'$.title') IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_traces_project_thread ON traces(project_id, thread_id, parent_span_id, start_time_us, finish_time_us);

WITH title_data AS (
    SELECT
        traces.thread_id,
        json_extract(message.value, '$.content') as message
    FROM traces,
         json_each(json(json_extract(attribute, '$.request')), '$.messages') message
    WHERE traces.operation_name = 'api_invoke'
        AND (
            json_extract(message.value, '$.role') != 'user'
            OR (
                json_type(message.value, '$.content') = 'text'
                AND json_extract(message.value, '$.content') NOT LIKE '[%'
                AND json_extract(message.value, '$.content') NOT LIKE '{{%'
            )
        )
    GROUP BY traces.thread_id
    HAVING MIN(traces.start_time_us) AND MIN(case when json_extract(message.value, '$.role') = 'user' then traces.start_time_us end)
)
UPDATE traces
SET attribute = json_set(COALESCE(attribute, '{}'), '$.title', 
    (SELECT message FROM title_data WHERE title_data.thread_id = traces.thread_id)
)
WHERE thread_id IN (SELECT thread_id FROM title_data)
  AND operation_name = 'api_invoke';

