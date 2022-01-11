ALTER TABLE sessions ADD CONSTRAINT unique_client_identifier UNIQUE (client_identifier);
