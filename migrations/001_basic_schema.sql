CREATE TABLE "main"."weekends"(
  "id" INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
  "name" TEXT NOT NULL,
  "series" TEXT NOT NULL,
  "start_date" TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d', CURRENT_TIMESTAMP)),
  "status" TEXT NOT NULL,
  "created_at" TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', CURRENT_TIMESTAMP))
);

CREATE TABLE "main"."sessions"(
  "id" INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
  "weekend_id" INTEGER NOT NULL,
  "start_time" TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', CURRENT_TIMESTAMP)),
  "name" TEXT NOT NULL,
  "duration" INTEGER NOT NULL DEFAULT '3600',
  "notify" TEXT NOT NULL,
  "status" TEXT NOT NULL,
  "created_at" TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', CURRENT_TIMESTAMP)),
  FOREIGN KEY ("weekend_id") REFERENCES "weekends" ("id")
);

CREATE TABLE "main"."messages"(
  "id" INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
  "message_discord_id" TEXT NOT NULL,
  "channel_discord_id" TEXT NOT NULL,
  "kind" TEXT NOT NULL,
  "series" TEXT NOT NULL,
  "expires_at" TEXT NOT NULL,
  "created_at" TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', CURRENT_TIMESTAMP))
);
