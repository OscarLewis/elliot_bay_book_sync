use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use tracing::debug;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResourcesRoot {
    #[serde(rename = "Resources")]
    pub resources: Resources,
}

impl Default for ResourcesRoot {
    fn default() -> Self {
        Self {
            resources: Resources::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Resources {
    pub account_page: String,
    pub account_page_rakuten: String,
    pub add_device: String,
    pub add_entitlement: String,
    pub affiliaterequest: String,
    pub assets: String,
    pub audiobook: String,
    pub audiobook_detail_page: String,
    pub audiobook_landing_page: String,
    pub audiobook_preview: String,
    pub audiobook_purchase_withcredit: String,
    pub audiobook_subscription_orange_deal_inclusion_url: String,
    pub authorproduct_recommendations: String,
    pub autocomplete: String,
    pub bam: String,
    pub blackstone_header: ResourcesHeader,
    pub book: String,
    pub book_detail_page: String,
    pub book_detail_page_rakuten: String,
    pub book_landing_page: String,
    pub book_subscription: String,
    pub browse_history: String,
    pub categories: String,
    pub categories_page: String,
    pub categoriesv2: String,
    pub category: String,
    pub category_featured_lists: String,
    pub category_products: String,
    pub checkout_borrowed_book: String,
    pub client_authd_referral: String,
    pub configuration_data: String,
    pub content_access_book: String,
    pub contributorsv2: String,
    pub createpurchaseifallowed_url: String,
    pub customer_care_live_chat: String,
    pub daily_deal: String,
    pub deals: String,
    pub delete_entitlement: String,
    pub delete_tag: String,
    pub delete_tag_items: String,
    pub delete_user_linked_accounts: String,
    pub device_auth: String,
    pub device_refresh: String,
    pub dictionary_host: String,
    pub discovery_host: String,
    pub display_accessibility_enabled: String,
    pub display_parental_controls_enabled: String,
    pub dropbox_link_account_poll: String,
    pub dropbox_link_account_start: String,
    pub elabel_url: String,
    pub ereaderdevices: String,
    pub eula_page: String,
    pub exchange_auth: String,
    pub external_book: String,
    pub facebook_sso_page: String,
    pub featured_list: String,
    #[serde(rename = "featuredlist2")]
    pub featured_list2: String,
    pub featured_lists: String,
    pub fixed_layout_page_cache_enabled: String,
    pub free_books_page: LocalizedPages,
    pub funnel_metrics: String,
    pub geography_data: String,
    pub get_download_keys: String,
    pub get_download_link: String,
    pub get_tests_request: String,
    pub giftcard_epd_redeem_url: String,
    pub giftcard_redeem_url: String,
    pub googledrive_link_account_start: String,
    pub gpb_flow_enabled: String,
    pub help_page: String,
    pub image_host: String,
    pub image_url_quality_template: String,
    pub image_url_template: String,
    pub instapaper_enabled: String,
    pub instapaper_env_url: String,
    pub instapaper_link_account_start: String,
    pub kobo_audiobooks_credit_redemption: String,
    pub kobo_audiobooks_enabled: String,
    pub kobo_audiobooks_orange_deal_enabled: String,
    pub kobo_audiobooks_subscriptions_enabled: String,
    pub kobo_display_price: String,
    pub kobo_dropbox_link_account_enabled: String,
    pub kobo_google_tax: String,
    pub kobo_googledrive_link_account_enabled: String,
    pub kobo_nativeborrow_enabled: String,
    pub kobo_onedrive_link_account_enabled: String,
    pub kobo_onestorelibrary_enabled: String,
    #[serde(rename = "kobo_privacyCentre_url")]
    pub kobo_privacy_centre_url: String,
    pub kobo_redeem_enabled: String,
    pub kobo_shelfie_enabled: String,
    pub kobo_shopping_cart_enabled: String,
    pub kobo_subscriptions_enabled: String,
    pub kobo_superpoints_enabled: String,
    pub kobo_wishlist_enabled: String,
    pub library_book: String,
    pub library_items: String,
    pub library_metadata: String,
    pub library_prices: String,
    pub library_search: String,
    pub library_sync: String,
    pub love_dashboard_page: String,
    pub love_points_redemption_page: String,
    pub magazine_landing_page: String,
    pub more_sign_in_options: String,
    pub morebyauthor: String,
    pub notebooks: String,
    pub notifications_registration_issue: String,
    pub oauth_host: String,
    pub optimus_enabled: String,
    pub overdrive_account: String,
    pub overdrive_library: String,
    pub overdrive_library_finder_host: String,
    pub overdrive_thunder_host: String,
    pub password_retrieval_page: String,
    pub patch_user_linked_accounts: String,
    pub personalizedrecommendations: String,
    pub pocket_link_account_start: String,
    pub post_analytics_event: String,
    pub ppx_purchasing_url: String,
    pub privacy_page: String,
    pub product_nextread: String,
    pub product_prices: String,
    pub product_recommendations: String,
    pub product_reviews: String,
    pub productbyid: String,
    pub productbyslug: String,
    pub products: String,
    pub productstatebyid: String,
    pub productstatebyslug: String,
    pub productsv2: String,
    pub provider_external_sign_in_page: String,
    pub purchase_buy: String,
    pub purchase_buy_templated: String,
    pub quickbuy_checkout: String,
    pub quickbuy_create: String,
    pub rakuten_token_exchange: String,
    pub rating: String,
    pub reading_services_host: String,
    pub reading_state: String,
    pub recommendations: String,
    pub redeem_interstitial_page: String,
    pub redeem_loyalty_points: String,
    pub reflowable_page_cache_enabled: String,
    pub registration_page: String,
    pub related: String,
    pub related_items: String,
    pub remaining_book_series: String,
    pub rename_tag: String,
    pub review: String,
    pub review_sentiment: String,
    pub sepa_banks: String,
    pub shelfie_recommendations: String,
    pub sign_in_page: String,
    pub social_authorization_host: String,
    pub social_host: String,
    pub store_home: String,
    pub store_host: String,
    pub store_newreleases: String,
    pub store_search: String,
    pub store_top50: String,
    pub subs_landing_page: String,
    pub subs_management_page: String,
    pub subs_plans_page: String,
    pub subs_purchase_buy_templated: String,
    pub tag_items: String,
    pub tags: String,
    pub terms_of_sale_page: String,
    pub text_to_speech_region_override: String,
    pub topproducts: String,
    pub tracking: String,
    pub update_accessibility_to_preview: String,
    pub use_one_store: String,
    pub user_currencyconversion: String,
    pub user_linked_accounts: String,
    pub user_loyalty_benefits: String,
    pub user_loyalty_membership: String,
    pub user_platform: String,
    pub user_profile: String,
    pub user_ratings: String,
    pub user_recommendations: String,
    pub user_reviews: String,
    pub user_subscription_koboplus: String,
    pub user_tasteprofile_complete: String,
    pub user_tasteprofile_genre: String,
    pub user_wishlist: String,
    pub userguide_host: String,
    pub wishlist_page: String,
    pub workbooks: String,

    #[serde(flatten)]
    pub extra_fields: HashMap<String, Value>,
}

impl Default for Resources {
    fn default() -> Self {
        Self {
            account_page: "https://www.kobo.com/account/settings".to_string(),
            account_page_rakuten: "https://my.rakuten.co.jp/".to_string(),
            add_device: "https://storeapi.kobo.com/v1/user/add-device".to_string(),
            add_entitlement: "https://storeapi.kobo.com/v1/library/{RevisionIds}".to_string(),
            affiliaterequest: "https://storeapi.kobo.com/v1/affiliate".to_string(),
            assets: "https://storeapi.kobo.com/v1/assets".to_string(),
            audiobook: "https://storeapi.kobo.com/v1/products/audiobooks/{ProductId}".to_string(),
            audiobook_detail_page: "https://www.kobo.com/{region}/{language}/audiobook/{slug}".to_string(),
            audiobook_landing_page: "https://www.kobo.com/{region}/{language}/audiobooks".to_string(),
            audiobook_preview: "https://storeapi.kobo.com/v1/products/audiobooks/{Id}/preview".to_string(),
            audiobook_purchase_withcredit: "https://storeapi.kobo.com/v1/store/audiobook/{Id}".to_string(),
            audiobook_subscription_orange_deal_inclusion_url: "https://authorize.kobo.com/inclusion".to_string(),
            authorproduct_recommendations: "https://storeapi.kobo.com/v1/products/books/authors/recommendations".to_string(),
            autocomplete: "https://storeapi.kobo.com/v1/products/autocomplete".to_string(),
            bam: "https://storeapi.kobo.com/v2/activity/bam/success".to_string(),
            blackstone_header: ResourcesHeader::default(),
            book: "https://storeapi.kobo.com/v1/products/books/{ProductId}".to_string(),
            book_detail_page: "https://www.kobo.com/{region}/{language}/ebook/{slug}".to_string(),
            book_detail_page_rakuten: "http://books.rakuten.co.jp/rk/{crossrevisionid}".to_string(),
            book_landing_page: "https://www.kobo.com/ebooks".to_string(),
            book_subscription: "https://storeapi.kobo.com/v1/products/books/subscriptions".to_string(),
            browse_history: "https://storeapi.kobo.com/v1/user/browsehistory".to_string(),
            categories: "https://storeapi.kobo.com/v1/categories".to_string(),
            categories_page: "https://www.kobo.com/ebooks/categories".to_string(),
            categoriesv2: "https://storeapi.kobo.com/api/v2/Categories/Top".to_string(),
            category: "https://storeapi.kobo.com/v1/categories/{CategoryId}".to_string(),
            category_featured_lists: "https://storeapi.kobo.com/v1/categories/{CategoryId}/featured".to_string(),
            category_products: "https://storeapi.kobo.com/v1/categories/{CategoryId}/products".to_string(),
            checkout_borrowed_book: "https://storeapi.kobo.com/v1/library/borrow".to_string(),
            client_authd_referral: "https://authorize.kobo.com/api/AuthenticatedReferral/client/v1/getLink".to_string(),
            configuration_data: "https://storeapi.kobo.com/v1/configuration".to_string(),
            content_access_book: "https://storeapi.kobo.com/v1/products/books/{ProductId}/access".to_string(),
            contributorsv2: "https://storeapi.kobo.com/v2/contributors/author".to_string(),
            createpurchaseifallowed_url: "https://www.kobo.com/checkout/createpurchaseifallowed".to_string(),
            customer_care_live_chat: "https://v2.zopim.com/widget/livechat.html?key=Y6gwUmnu4OATxN3Tli4Av9bYN319BTdO".to_string(),
            daily_deal: "https://storeapi.kobo.com/v1/products/dailydeal".to_string(),
            deals: "https://storeapi.kobo.com/v1/deals".to_string(),
            delete_entitlement: "https://storeapi.kobo.com/v1/library/{Ids}".to_string(),
            delete_tag: "https://storeapi.kobo.com/v1/library/tags/{TagId}".to_string(),
            delete_tag_items: "https://storeapi.kobo.com/v1/library/tags/{TagId}/items/delete".to_string(),
            delete_user_linked_accounts: "https://storeapi.kobo.com/v1/user/linkedaccounts/{Id}".to_string(),
            device_auth: "https://storeapi.kobo.com/v1/auth/device".to_string(),
            device_refresh: "https://storeapi.kobo.com/v1/auth/refresh".to_string(),
            dictionary_host: "https://ereaderfiles.kobo.com".to_string(),
            discovery_host: "https://discovery.kobobooks.com".to_string(),
            display_accessibility_enabled: "False".to_string(),
            display_parental_controls_enabled: "False".to_string(),
            dropbox_link_account_poll: "https://authorize.kobo.com/{region}/{language}/LinkDropbox".to_string(),
            dropbox_link_account_start: "https://authorize.kobo.com/LinkDropbox/start".to_string(),
            elabel_url: "https://ereaderfiles.kobo.com/elabels/".to_string(),
            ereaderdevices: "https://storeapi.kobo.com/v2/products/EReaderDeviceFeeds".to_string(),
            eula_page: "https://www.kobo.com/termsofuse?style=onestore".to_string(),
            exchange_auth: "https://storeapi.kobo.com/v1/auth/exchange".to_string(),
            external_book: "https://storeapi.kobo.com/v1/products/books/external/{Ids}".to_string(),
            facebook_sso_page: "https://authorize.kobo.com/signin/provider/Facebook/login?returnUrl=https://kobo.com/".to_string(),
            featured_list: "https://storeapi.kobo.com/v1/products/featured/{FeaturedListId}".to_string(),
            featured_list2: "https://storeapi.kobo.com/v2/products/list/featured".to_string(),
            featured_lists: "https://storeapi.kobo.com/v1/products/featured".to_string(),
            fixed_layout_page_cache_enabled: "True".to_string(),
            free_books_page: LocalizedPages::default(),
            funnel_metrics: "https://storeapi.kobo.com/v1/funnelmetrics".to_string(),
            geography_data: "https://storeapi.kobo.com/v2/configuration/geography/country".to_string(),
            get_download_keys: "https://storeapi.kobo.com/v1/library/downloadkeys".to_string(),
            get_download_link: "https://storeapi.kobo.com/v1/library/downloadlink".to_string(),
            get_tests_request: "https://storeapi.kobo.com/v1/analytics/gettests".to_string(),
            giftcard_epd_redeem_url: "https://www.kobo.com/{storefront}/{language}/redeem-ereader".to_string(),
            giftcard_redeem_url: "https://www.kobo.com/{storefront}/{language}/redeem".to_string(),
            googledrive_link_account_start: "https://authorize.kobo.com/{region}/{language}/linkcloudstorage/provider/google_drive".to_string(),
            gpb_flow_enabled: "False".to_string(),
            help_page: "https://www.kobo.com/help".to_string(),
            image_host: "//cdn.kobo.com/book-images/".to_string(),
            image_url_quality_template: "https://cdn.kobo.com/book-images/{ImageId}/{Width}/{Height}/{Quality}/{IsGreyscale}/image.jpg".to_string(),
            image_url_template: "https://cdn.kobo.com/book-images/{ImageId}/{Width}/{Height}/false/image.jpg".to_string(),
            instapaper_enabled: "True".to_string(),
            instapaper_env_url: "https://www.instapaper.com/api/kobo".to_string(),
            instapaper_link_account_start: "https://authorize.kobo.com/{region}/{language}/linkinstapaper".to_string(),
            kobo_audiobooks_credit_redemption: "True".to_string(),
            kobo_audiobooks_enabled: "True".to_string(),
            kobo_audiobooks_orange_deal_enabled: "False".to_string(),
            kobo_audiobooks_subscriptions_enabled: "False".to_string(),
            kobo_display_price: "True".to_string(),
            kobo_dropbox_link_account_enabled: "True".to_string(),
            kobo_google_tax: "False".to_string(),
            kobo_googledrive_link_account_enabled: "True".to_string(),
            kobo_nativeborrow_enabled: "True".to_string(),
            kobo_onedrive_link_account_enabled: "False".to_string(),
            kobo_onestorelibrary_enabled: "False".to_string(),
            kobo_privacy_centre_url: "https://www.kobo.com/privacy".to_string(),
            kobo_redeem_enabled: "True".to_string(),
            kobo_shelfie_enabled: "False".to_string(),
            kobo_shopping_cart_enabled: "False".to_string(),
            kobo_subscriptions_enabled: "True".to_string(),
            kobo_superpoints_enabled: "True".to_string(),
            kobo_wishlist_enabled: "True".to_string(),
            library_book: "https://storeapi.kobo.com/v1/user/library/books/{LibraryItemId}".to_string(),
            library_items: "https://storeapi.kobo.com/v1/user/library".to_string(),
            library_metadata: "https://storeapi.kobo.com/v1/library/{Ids}/metadata".to_string(),
            library_prices: "https://storeapi.kobo.com/v1/user/library/previews/prices".to_string(),
            library_search: "https://storeapi.kobo.com/v1/library/search".to_string(),
            library_sync: "https://storeapi.kobo.com/v1/library/sync".to_string(),
            love_dashboard_page: "https://www.kobo.com/{region}/{language}/kobosuperpoints".to_string(),
            love_points_redemption_page: "https://www.kobo.com/{region}/{language}/KoboSuperPointsRedemption?productId={ProductId}".to_string(),
            magazine_landing_page: "https://www.kobo.com/emagazines".to_string(),
            more_sign_in_options: "https://authorize.kobo.com/signin?returnUrl=https://kobo.com/#allProviders".to_string(),
            morebyauthor: "https://storeapi.kobo.com/v2/products/recommendations/morebyauthor".to_string(),
            notebooks: "https://storeapi.kobo.com/api/internal/notebooks".to_string(),
            notifications_registration_issue: "https://storeapi.kobo.com/v1/notifications/registration".to_string(),
            oauth_host: "https://oauth.kobo.com".to_string(),
            optimus_enabled: "False".to_string(),
            overdrive_account: "https://auth.overdrive.com/account".to_string(),
            overdrive_library: "https://{libraryKey}.auth.overdrive.com/library".to_string(),
            overdrive_library_finder_host: "https://libraryfinder.api.overdrive.com".to_string(),
            overdrive_thunder_host: "https://thunder.api.overdrive.com".to_string(),
            password_retrieval_page: "https://www.kobo.com/passwordretrieval.html".to_string(),
            patch_user_linked_accounts: "https://storeapi.kobo.com/v1/user/linkedaccounts/{Id}".to_string(),
            personalizedrecommendations: "https://storeapi.kobo.com/v2/users/personalizedrecommendations".to_string(),
            pocket_link_account_start: "https://authorize.kobo.com/{region}/{language}/linkpocket".to_string(),
            post_analytics_event: "https://storeapi.kobo.com/v1/analytics/event".to_string(),
            ppx_purchasing_url: "https://purchasing.kobo.com".to_string(),
            privacy_page: "https://www.kobo.com/privacypolicy?style=onestore".to_string(),
            product_nextread: "https://storeapi.kobo.com/v1/products/{ProductIds}/nextread".to_string(),
            product_prices: "https://storeapi.kobo.com/v1/products/{ProductIds}/prices".to_string(),
            product_recommendations: "https://storeapi.kobo.com/v1/products/{ProductId}/recommendations".to_string(),
            product_reviews: "https://storeapi.kobo.com/v1/products/{ProductIds}/reviews".to_string(),
            productbyid: "https://storeapi.kobo.com/v2/products/itemDetailById/{ProductType}/{Id}".to_string(),
            productbyslug: "https://storeapi.kobo.com/v2/products/itemDetail/{ProductType}/{Slug}".to_string(),
            products: "https://storeapi.kobo.com/v1/products".to_string(),
            productstatebyid: "https://storeapi.kobo.com/v2/products/itemStateById/{ProductType}/{Id}".to_string(),
            productstatebyslug: "https://storeapi.kobo.com/v2/products/itemState/{ProductType}/{Slug}".to_string(),
            productsv2: "https://storeapi.kobo.com/v2/products".to_string(),
            provider_external_sign_in_page: "https://authorize.kobo.com/ExternalSignIn/{providerName}?returnUrl=https://kobo.com/".to_string(),
            purchase_buy: "https://www.kobo.com/checkoutoption/".to_string(),
            purchase_buy_templated: "https://www.kobo.com/{region}/{language}/checkoutoption/{ProductId}".to_string(),
            quickbuy_checkout: "https://storeapi.kobo.com/v1/store/quickbuy/{PurchaseId}/checkout".to_string(),
            quickbuy_create: "https://storeapi.kobo.com/v1/store/quickbuy/purchase".to_string(),
            rakuten_token_exchange: "https://storeapi.kobo.com/v1/auth/rakuten_token_exchange".to_string(),
            rating: "https://storeapi.kobo.com/v1/products/{ProductId}/rating/{Rating}".to_string(),
            reading_services_host: "https://readingservices.kobo.com".to_string(),
            reading_state: "https://storeapi.kobo.com/v1/library/{Ids}/state".to_string(),
            recommendations: "https://storeapi.kobo.com/v1/products/bulk".to_string(),
            redeem_interstitial_page: "https://www.kobo.com".to_string(),
            redeem_loyalty_points: "https://storeapi.kobo.com/v1/user/loyalty/redeem".to_string(),
            reflowable_page_cache_enabled: "True".to_string(),
            registration_page: "https://authorize.kobo.com/signup?returnUrl=https://kobo.com/".to_string(),
            related: "https://storeapi.kobo.com/v2/products/recommendations/related".to_string(),
            related_items: "https://storeapi.kobo.com/v1/products/{Id}/related".to_string(),
            remaining_book_series: "https://storeapi.kobo.com/v1/products/books/series/{SeriesId}".to_string(),
            rename_tag: "https://storeapi.kobo.com/v1/library/tags/{TagId}".to_string(),
            review: "https://storeapi.kobo.com/v1/products/reviews/{ReviewId}".to_string(),
            review_sentiment: "https://storeapi.kobo.com/v1/products/reviews/{ReviewId}/sentiment/{Sentiment}".to_string(),
            sepa_banks: "https://storeapi.kobo.com/v2/purchasing/sepa/banks".to_string(),
            shelfie_recommendations: "https://storeapi.kobo.com/v1/user/recommendations/shelfie".to_string(),
            sign_in_page: "https://auth.kobobooks.com/ActivateOnWeb".to_string(),
            social_authorization_host: "https://social.kobobooks.com:8443".to_string(),
            social_host: "https://social.kobobooks.com".to_string(),
            store_home: "www.kobo.com/{region}/{language}".to_string(),
            store_host: "www.kobo.com".to_string(),
            store_newreleases: "https://www.kobo.com/{region}/{language}/List/new-releases/961XUjtsU0qxkFItWOutGA".to_string(),
            store_search: "https://www.kobo.com/{region}/{language}/Search?Query={query}".to_string(),
            store_top50: "https://www.kobo.com/{region}/{language}/ebooks/Top".to_string(),
            subs_landing_page: "https://www.kobo.com/{region}/{language}/plus".to_string(),
            subs_management_page: "https://www.kobo.com/{region}/{language}/account/subscriptions".to_string(),
            subs_plans_page: "https://www.kobo.com/{region}/{language}/plus/plans".to_string(),
            subs_purchase_buy_templated: "https://www.kobo.com/{region}/{language}/Checkoutoption/{ProductId}/{TierId}".to_string(),
            tag_items: "https://storeapi.kobo.com/v1/library/tags/{TagId}/Items".to_string(),
            tags: "https://storeapi.kobo.com/v1/library/tags".to_string(),
            terms_of_sale_page: "https://authorize.kobo.com/{region}/{language}/terms/termsofsale".to_string(),
            text_to_speech_region_override: "False".to_string(),
            topproducts: "https://storeapi.kobo.com/v2/products/list/topproducts".to_string(),
            tracking: "https://storeapi.kobo.com/v2/tracking/searchperformed".to_string(),
            update_accessibility_to_preview: "https://storeapi.kobo.com/v1/library/{EntitlementIds}/preview".to_string(),
            use_one_store: "True".to_string(),
            user_currencyconversion: "https://storeapi.kobo.com/v1/user/currency/convert".to_string(),
            user_linked_accounts: "https://storeapi.kobo.com/v1/user/linkedaccounts".to_string(),
            user_loyalty_benefits: "https://storeapi.kobo.com/v1/user/loyalty/benefits".to_string(),
            user_loyalty_membership: "https://storeapi.kobo.com/v1/user/loyalty/membership".to_string(),
            user_platform: "https://storeapi.kobo.com/v1/user/platform".to_string(),
            user_profile: "https://storeapi.kobo.com/v1/user/profile".to_string(),
            user_ratings: "https://storeapi.kobo.com/v1/user/ratings".to_string(),
            user_recommendations: "https://storeapi.kobo.com/v1/user/recommendations".to_string(),
            user_reviews: "https://storeapi.kobo.com/v1/user/reviews".to_string(),
            user_subscription_koboplus: "https://storeapi.kobo.com/v1/user/subscription/kp/state".to_string(),
            user_tasteprofile_complete: "https://storeapi.kobo.com/v2/user/tasteprofile/complete".to_string(),
            user_tasteprofile_genre: "https://storeapi.kobo.com/v2/user/tasteprofile/genre".to_string(),
            user_wishlist: "https://storeapi.kobo.com/v1/user/wishlist".to_string(),
            userguide_host: "https://ereaderfiles.kobo.com".to_string(),
            wishlist_page: "https://www.kobo.com/{region}/{language}/account/wishlist".to_string(),
            workbooks: "https://storeapi.kobo.com/v2/products/workbooks".to_string(),
            extra_fields: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResourcesHeader {
    pub key: String,
    pub value: String,
}

impl Default for ResourcesHeader {
    fn default() -> Self {
        Self {
            key: "x-amz-request-payer".to_string(),
            value: "requester".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LocalizedPages {
    #[serde(rename = "EN")]
    pub en: String,
    #[serde(rename = "FR")]
    pub fr: String,
    #[serde(rename = "IT")]
    pub it: String,
    #[serde(rename = "NL")]
    pub nl: String,
    #[serde(rename = "PT")]
    pub pt: String,
}

impl Default for LocalizedPages {
    fn default() -> Self {
        Self {
            en: "https://www.kobo.com/{region}/{language}/p/free-ebooks".to_string(),
            fr: "https://www.kobo.com/{region}/{language}/p/livres-gratuits".to_string(),
            it: "https://www.kobo.com/{region}/{language}/p/libri-gratuiti".to_string(),
            nl: "https://www.kobo.com/{region}/{language}/List/bekijk-het-overzicht-van-gratis-ebooks/QpkkVWnUw8sxmgjSlCbJRg".to_string(),
            pt: "https://www.kobo.com/{region}/{language}/p/livros-gratis".to_string(),
        }
    }
}

pub fn patch_kobo_resources(
    resources: Resources,
    base_url: &str, // e.g. "https://books.example.com" or "http://192.168.1.50:8083"
    auth_token: &str,
    is_kobo_proxy_enabled: bool,
) -> Resources {
    let clean_base = base_url.trim_end_matches('/');

    let mut patched_resources = resources.clone();

    // Rewrite Cover Image Endpoints
    patched_resources.image_host = clean_base.to_string();

    let quality_url = format!(
        "{clean_base}/kobo/{auth_token}/cover/{{ImageId}}/{{width}}/{{height}}/{{Quality}}/isGreyscale"
    );
    patched_resources.image_url_quality_template = quality_url;

    let standard_url =
        format!("{clean_base}/kobo/{auth_token}/cover/{{ImageId}}/{{width}}/{{height}}/false");
    patched_resources.image_url_template = standard_url;

    // Fallbacks when not proxying the official Kobo Store
    if !is_kobo_proxy_enabled {
        patched_resources.oauth_host = format!("{clean_base}/kobo/{auth_token}/oauth");
    }

    debug!(
        original.image_host = %resources.image_host,
        patched.image_host = %patched_resources.image_host,
        original.image_url_quality_template = %resources.image_url_quality_template,
        patched.image_url_quality_template = %patched_resources.image_url_quality_template,
        original.image_url_template = %resources.image_url_template,
        patched.image_url_template = %patched_resources.image_url_template,
        original.oauth_host = %resources.oauth_host,
        patched.oauth_host = %patched_resources.oauth_host,
        is_kobo_proxy_enabled = is_kobo_proxy_enabled,
        "Patched Kobo resources configuration"
    );

    patched_resources
}

#[cfg(test)]
mod tests {
    use crate::{config::AppConfig, test_helpers::AppTestContext};
    use test_context::test_context;
    use test_log::test;

    #[test_context(AppTestContext)]
    #[test(tokio::test)]
    async fn test_app_state_patched_resources_differ_from_defaults(
        ctx: &mut AppTestContext,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let test_base_url = "https://books.example.com/";
        let test_auth_key = "test-token-123";

        // let config = AppConfig {
        //     base_url: test_base_url.to_string(),
        //     ebbooks_auth_key: test_auth_key.to_string(),
        //     proxy_kobo_store: false,
        //     ..AppConfig::default()
        // };

        // let ctx = MongoTestContext::setup_with_config(config).await;
        // let state = ctx.state.clone();

        let config = AppConfig {
            base_url: test_base_url.to_string(),
            ebbooks_auth_key: test_auth_key.to_string(),
            proxy_kobo_store: false,
            ..AppConfig::default()
        };

        ctx.set_config(config).await;

        // Lock both resource instances for comparison
        let original = ctx.state.kobo_resources.lock().await;
        let patched = ctx.state.patched_resources.lock().await;

        let clean_base = test_base_url.trim_end_matches('/');

        // Image host should be updated to base_url without trailing slash
        assert_ne!(original.image_host, patched.image_host);
        assert_eq!(patched.image_host, clean_base);

        // Image templates should include base_url and auth_token
        assert_ne!(
            original.image_url_quality_template,
            patched.image_url_quality_template
        );
        assert!(patched.image_url_quality_template.contains(clean_base));
        assert!(patched.image_url_quality_template.contains(test_auth_key));

        assert_ne!(original.image_url_template, patched.image_url_template);
        assert!(patched.image_url_template.contains(clean_base));
        assert!(patched.image_url_template.contains(test_auth_key));

        // OAuth host should be patched when proxy_kobo_store is false
        assert_ne!(original.oauth_host, patched.oauth_host);
        assert_eq!(
            patched.oauth_host,
            format!("{clean_base}/kobo/{test_auth_key}/oauth")
        );

        Ok(())
    }
}
