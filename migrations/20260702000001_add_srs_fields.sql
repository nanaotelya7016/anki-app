-- SRS(間隔反復学習)用フィールドをcardsテーブルに追加
-- easiness   : SM-2の難易度係数(デフォルト2.5)
-- interval   : 次回出題までの日数(デフォルト1日)
-- repetitions: 連続正解回数
-- next_review : 次回出題日(デフォルト今日=即出題対象)

ALTER TABLE cards ADD COLUMN easiness    DOUBLE PRECISION NOT NULL DEFAULT 2.5;
ALTER TABLE cards ADD COLUMN interval    INTEGER          NOT NULL DEFAULT 1;
ALTER TABLE cards ADD COLUMN repetitions INTEGER          NOT NULL DEFAULT 0;
ALTER TABLE cards ADD COLUMN next_review DATE             NOT NULL DEFAULT CURRENT_DATE;
