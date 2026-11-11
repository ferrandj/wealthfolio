//! MCP tools for the rate tables (savings, loans, tax): read them, edit one, and confirm the half-yearly review.

use serde::Deserialize;
use serde_json::json;
use wealthfolio_core::rates::RateInput;

use super::service_tools::{fail, need, no_args, object, reply, service_tool};

service_tool!(
    ListRates, "list_rates", Read, additive = false,
    scopes = [RatesRead],
    description = "The rates in use: savings rates, loan rates and the tax rates, with whether each is gross, tax exempt and where it comes from.",
    schema = no_args(),
    |env, _args| {
        reply(&need(env.rates_service(), "Rates")?.list().await.map_err(fail)?)
    }
);

service_tool!(
    GetRatesReview, "get_rates_review", Read, additive = false,
    scopes = [RatesRead],
    description = "Whether the rate tables must be confirmed again (they are revised on 1 February and 1 August), and when they were last confirmed.",
    schema = no_args(),
    |env, _args| {
        reply(&need(env.rates_service(), "Rates")?.review(chrono::Utc::now().date_naive()).await.map_err(fail)?)
    }
);

service_tool!(
    SaveRate, "save_rate", Write, additive = false,
    scopes = [RatesWrite],
    description = "Change a rate (rateId from list_rates). For an account rate that does not exist yet, give accountId: the rate is created. A field left out stays as it is.",
    schema = object(json!({
        "rateId": {"type": "string"},
        "label": {"type": ["string", "null"]},
        "rate": {"type": ["string", "number", "null"], "description": "In percent, e.g. 3 or \"1.5\"."},
        "gross": {"type": ["boolean", "null"]},
        "taxExempt": {"type": ["boolean", "null"]},
        "taxType": {"type": ["string", "null"], "description": "Account rates: tax.type.*; an empty string goes back to the flat tax."},
        "source": {"type": ["string", "null"]},
        "accountId": {"type": ["string", "null"]}
    }), &["rateId"]),
    |env, args| {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Args {
            rate_id: String,
            #[serde(flatten)]
            input: RateInput,
        }
        let args: Args = serde_json::from_value(args)?;
        reply(&need(env.rates_service(), "Rates")?.save(&args.rate_id, args.input).await.map_err(fail)?)
    }
);

service_tool!(
    DeleteRate,
    "delete_rate",
    Write,
    additive = false,
    scopes = [RatesWrite],
    description = "Delete a rate you added (rateId from list_rates).",
    schema = object(json!({"rateId": {"type": "string"}}), &["rateId"]),
    |env, args| {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Args {
            rate_id: String,
        }
        let args: Args = serde_json::from_value(args)?;
        need(env.rates_service(), "Rates")?
            .delete(&args.rate_id)
            .await
            .map_err(fail)?;
        reply(&json!({"deleted": args.rate_id}))
    }
);

service_tool!(
    ConfirmRates, "confirm_rates", Write, additive = false,
    scopes = [RatesWrite],
    description = "Confirm that the rate tables are up to date after the half-yearly revision. Returns the new review state.",
    schema = no_args(),
    |env, _args| {
        reply(&need(env.rates_service(), "Rates")?.confirm(chrono::Utc::now().date_naive()).await.map_err(fail)?)
    }
);
