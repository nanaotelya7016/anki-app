-- decksテーブル
-- Phase 1では認証なし・固定ユーザー運用なので user_id カラムは持たない
CREATE TABLE decks (
    id         BIGSERIAL    PRIMARY KEY,           -- 自動採番のID(1, 2, 3...)
    name       TEXT         NOT NULL,              -- デッキ名(空文字禁止)
    created_at TIMESTAMPTZ  NOT NULL DEFAULT NOW() -- 作成日時(タイムゾーン付き)
);

-- cardsテーブル
-- deck_id で decks テーブルと紐付ける(外部キー)
CREATE TABLE cards (
    id         BIGSERIAL    PRIMARY KEY,
    deck_id    BIGINT       NOT NULL REFERENCES decks(id) ON DELETE CASCADE,
    -- ON DELETE CASCADE: デッキを削除したら紐づくカードも自動削除
    front      TEXT         NOT NULL,              -- カード表面
    back       TEXT         NOT NULL,              -- カード裏面
    created_at TIMESTAMPTZ  NOT NULL DEFAULT NOW()
);
