import copy
import datetime
import unittest
from policy import license_allowed, check_components, check_npm_audit, check_rust_report, validate_reviews, advisory_allowed

BASE={'allowedLicenses':['MIT','Apache-2.0'],'reviewLicenses':['MPL-2.0'],'reviews':[],'advisoryExceptions':[],'securityCritical':['rustls']}
COMP={'ref':'cargo:example@1.0.0','ecosystem':'cargo','name':'example','version':'1.0.0','license':'MIT OR Apache-2.0','source':'registry+https://github.com/rust-lang/crates.io-index','checksum':'123','direct':False}
# Fixed synthetic advisory/database revision: fixtures never fetch a network DB.
DB_REVISION='0123456789abcdef0123456789abcdef01234567'

class PolicyTests(unittest.TestCase):
    def test_license_expression_is_not_a_substring_allowlist(self):
        for license in ['MIT OR GPL-3.0-only','(MIT OR Apache-2.0) AND MIT','MIT/Apache-2.0']:
            self.assertTrue(license_allowed(license,BASE['allowedLicenses']))
        for license in ['',None,'MIT AND GPL-3.0-only','MIT OR','UNKNOWN','MITgarbage','MIT WITH unknown','(MIT','MIT)']:
            self.assertFalse(license_allowed(license,BASE['allowedLicenses']))
    def test_unknown_prohibited_git_path_scripts_and_new_direct_fail(self):
        self.assertEqual(check_components([COMP],BASE)[0],[])
        for change in [{'license':'GPL-3.0-only'},{'license':'UNKNOWN'},{'source':'git+https://example.test/repo'},{'source':'path:vendor/new'},{'installScript':True},{'direct':True}]:
            self.assertTrue(check_components([{**COMP,**change}],BASE)[0])
    def test_reviews_are_scoped_fingerprinted_and_expire(self):
        p=copy.deepcopy(BASE);p['reviews']=[{'kind':'source','key':COMP['ref'],'fingerprint':'123','owner':'test','reason':'fixture','expires':'2000-01-01'}]
        self.assertTrue(validate_reviews(p))
        p['reviews'][0]['expires']='2999-01-01'
        self.assertFalse(check_components([{**COMP,'source':'path:vendor/old'}],p)[0])
        self.assertTrue(check_components([{**COMP,'source':'path:vendor/old','checksum':'changed'}],p)[0])
    def test_critical_duplicates_need_exact_version_review(self):
        cs=[{**COMP,'name':'rustls','version':v,'ref':'cargo:rustls@'+v} for v in ['0.22','0.23']]
        errors,duplicates=check_components(cs,BASE)
        self.assertTrue(errors);self.assertEqual(duplicates[0]['versions'],['0.22','0.23'])
    def test_npm_vulnerable_fixture_and_scanner_errors_block(self):
        c={**COMP,'ecosystem':'npm','name':'example','lockPath':'node_modules/example'}
        r={'metadata':{},'vulnerabilities':{'example':{'severity':'high','nodes':['node_modules/example'],'via':[{'source':123}]}}}
        self.assertTrue(check_npm_audit(r,[c],BASE));self.assertTrue(check_npm_audit({'error':'offline'},[c],BASE))
    def test_rust_vulnerability_yank_and_expired_exception_block(self):
        r=[{'type':'diagnostic','fields':{'severity':'error','code':'vulnerability','advisory':{'id':'RUSTSEC-2099-0001','title':'fixture'},'graphs':[{'Krate':{'name':'example','version':'1.0.0'}}]}},{'type':'summary','fields':{'advisories':{'errors':1}}}]
        self.assertTrue(check_rust_report(r,BASE)[0])
        p=copy.deepcopy(BASE);p['advisoryExceptions']=[{'key':'cargo:example@1.0.0:RUSTSEC-2099-0001','owner':'test','reason':'fixture','control':'test-only','expires':'2999-01-01'}]
        self.assertFalse(check_rust_report(r,p)[0]);self.assertFalse(advisory_allowed(p,'cargo','example','2.0.0','RUSTSEC-2099-0001'))
        p['advisoryExceptions'][0]['expires']='2000-01-01';self.assertTrue(check_rust_report(r,p)[0])
        r[0]['fields'].pop('advisory');r[0]['fields']['code']='yanked';self.assertTrue(check_rust_report(r,p)[0])
        self.assertTrue(check_rust_report([],BASE)[0]);r[1]['fields']['advisories']['errors']=2;self.assertTrue(check_rust_report(r,BASE)[0])

if __name__=='__main__':unittest.main()
