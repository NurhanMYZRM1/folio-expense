use folio::{domain::tax::*, error::Result, security::secrets::SecretStore, services::AppService};
use std::sync::Arc;
use zeroize::Zeroizing;
struct NoSecrets;
impl SecretStore for NoSecrets {
    fn get(&self) -> Result<Option<Zeroizing<String>>> {
        Ok(None)
    }
    fn set(&self, _: &str) -> Result<()> {
        Ok(())
    }
    fn delete(&self) -> Result<()> {
        Ok(())
    }
}
fn service() -> (tempfile::TempDir, AppService) {
    let dir = tempfile::tempdir().unwrap();
    let service = AppService::open(dir.path().join("app"), Arc::new(NoSecrets)).unwrap();
    (dir, service)
}
fn input(total: i64) -> IncomeTaxInput {
    IncomeTaxInput {
        assessment_year: 2025,
        resident: true,
        marital_status: MaritalStatus::Single,
        total_income_minor: total,
        reliefs: vec![],
        zakat_minor: 0,
        pcb_paid_minor: 0,
    }
}
#[test]
fn official_ya2025_form_be_example() {
    let (_dir, s) = service();
    let result = s.estimate_income_tax(input(5_450_000)).unwrap();
    assert_eq!(result.relief_minor, 900_000);
    assert_eq!(result.chargeable_income_minor, 4_550_000);
    assert_eq!(result.tax_before_rebates_minor, 123_000);
    assert_eq!(result.tax_payable_minor, 123_000);
}
#[test]
fn rebate_and_pcb_do_not_create_negative_tax() {
    let (_dir, s) = service();
    let mut i = input(4_300_000);
    i.marital_status = MaritalStatus::MarriedSpouseNoIncome;
    i.zakat_minor = 100_000;
    i.pcb_paid_minor = 25_000;
    let result = s.estimate_income_tax(i).unwrap();
    assert_eq!(result.tax_payable_minor, 0);
    assert_eq!(result.balance_after_pcb_minor, -25_000);
    assert_eq!(result.rebate_minor, result.tax_before_rebates_minor);
}
#[test]
fn unsupported_year_nonresident_and_unverified_relief_are_refused() {
    let (_dir, s) = service();
    assert!(s.get_tax_rules(2026).is_err());
    let mut i = input(1_000_000);
    i.resident = false;
    assert!(s.estimate_income_tax(i).is_err());
    let mut i = input(5_000_000);
    i.reliefs = vec![TaxReliefClaim {
        code: "epf".into(),
        amount_minor: 100,
        eligibility_confirmed: false,
    }];
    assert!(s.estimate_income_tax(i).is_err());
}
#[test]
fn relief_caps_duplicates_and_threshold_boundary() {
    let (_dir, s) = service();
    let mut i = input(5_000_000);
    i.reliefs = vec![TaxReliefClaim {
        code: "epf".into(),
        amount_minor: 800_000,
        eligibility_confirmed: true,
    }];
    assert_eq!(
        s.estimate_income_tax(i.clone()).unwrap().relief_minor,
        1_300_000
    );
    i.reliefs.push(i.reliefs[0].clone());
    assert!(s.estimate_income_tax(i).is_err());
    let low = s.estimate_income_tax(input(4_400_000)).unwrap();
    assert_eq!(low.rebate_minor, 40_000);
    let high = s.estimate_income_tax(input(4_400_100)).unwrap();
    assert_eq!(high.rebate_minor, 0);
}
#[test]
fn sst_inclusive_exclusive_halfup_and_limits() {
    let (_dir, s) = service();
    let a = s
        .calculate_sst(SstInput {
            amount_minor: 1001,
            rate_basis_points: 600,
            tax_inclusive: false,
        })
        .unwrap();
    assert_eq!((a.net_minor, a.tax_minor, a.total_minor), (1001, 60, 1061));
    let b = s
        .calculate_sst(SstInput {
            amount_minor: 10800,
            rate_basis_points: 800,
            tax_inclusive: true,
        })
        .unwrap();
    assert_eq!(
        (b.net_minor, b.tax_minor, b.total_minor),
        (10000, 800, 10800)
    );
    assert_eq!(
        s.calculate_sst(SstInput {
            amount_minor: 10,
            rate_basis_points: 500,
            tax_inclusive: false
        })
        .unwrap()
        .tax_minor,
        1
    );
    assert!(s
        .calculate_sst(SstInput {
            amount_minor: 999_999_999_999,
            rate_basis_points: 1000,
            tax_inclusive: false
        })
        .is_err());
}
