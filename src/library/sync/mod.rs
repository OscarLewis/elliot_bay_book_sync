pub mod delete_library_item_handler;
pub mod download_handler;
pub mod entitlement_models;
pub mod get_tests;
pub mod metadata_handler;
pub mod reading_state_handler;
pub mod stubs;
pub mod sync_document;
pub mod sync_handler;
pub mod sync_token;

/*
@csrf.exempt
@kobo.route("/v1/library/<book_uuid>", methods=["DELETE"])
@requires_kobo_auth
def HandleBookDeletionRequest(book_uuid):
    log.info("Kobo book delete request received for book %s", book_uuid)
    book = calibre_db.get_book_by_uuid(book_uuid)
    if not book:
        log.info("Book %s not found in database", book_uuid)
        return redirect_or_proxy_request()

    book_id = book.id
    # If the user has shelf sync enabled, do nothing.
    # The book will be removed from the device on the next sync.
    if current_user.kobo_only_shelves_sync:
        pass
    # Otherwise, archive the book if the user has permission to see archived books.
    elif current_user.check_visibility(32768):
        kobo_sync_status.change_archived_books(book_id, True)

    kobo_sync_status.remove_synced_book(book_id)
    return "", 204
*/
