import importlib.util
from pathlib import Path
import sys
import types
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('pms_plugin_test', Path(__file__).parents[1] / '__init__.py')
plugin = importlib.util.module_from_spec(spec)
spec.loader.exec_module(plugin)

class PluginTests(unittest.TestCase):
    def test_manual_history_is_host_only_and_handoff_reads_before_registration(self):
        for name in ('handoff','reconcile'):
            with self.assertRaises(ValueError): plugin.validate(name,{'action':'a','readback':{}})
        calls=[]
        observed={'order':{'id':'o','property_id':'p','version':4,'phone':'private'},
            'amendments':[{'id':'h','payload':{'exact':'effect'},'reason_note':'private','actor':{'displayName':'private'}}],
            'collectionFacts':[{'fact_id':'f','order_id':'o','transaction_reference':'ref','raw':'private'}]}
        class Pms:
            def read(self,kind,prop,resource):
                calls.append(('read',kind,prop,resource));return observed
        def broker(r):
            calls.append(r['tool'])
            if r['tool'].endswith('_context'): return {'ok':True,'result':{'property':'p','order_ref':'o'}}
            self.assertEqual(r['arguments']['readback']['amendments'],[{'id':'h','payload':{'exact':'effect'}}])
            return {'ok':True,'result':{'phase':'manual_completed','readback':r['arguments']['readback']}}
        op=plugin.Operations(Pms(),broker,lambda:{})
        for name in ('handoff','reconcile'):
            calls.clear()
            result=op.invoke(name,{'action':'a'})
            self.assertEqual(calls[1],('read','order','p','o'))
            self.assertNotIn('amendments',result['result']['readback'])
            self.assertNotIn('collectionFacts',result['result']['readback'])
            self.assertNotIn('phone',result['result']['readback']['order'])

    def test_link_reads_host_selected_current_order_and_rejects_model_authority(self):
        for extra in ({'readback':{}},{'approved':True},{'text':'确认关联'}):
            with self.assertRaises(ValueError):
                plugin.validate('link',{'action':'a','work_item':'s',**extra})
        calls=[]
        class Pms:
            def read(self,kind,prop,resource):
                self.seen=(kind,prop,resource)
                return {'order':{'id':resource,'property_id':prop,'version':3,'phone':'private'}}
        def broker(r):
            calls.append(r)
            if r['tool']=='pms_link_context': return {'ok':True,'result':{'property':'p','order_ref':'trusted-order'}}
            return {'ok':True,'result':{'phase':'awaiting_confirmation'}}
        pms=Pms()
        result=plugin.Operations(pms,broker,lambda:{}).link({'action':'a','work_item':'s'})
        self.assertEqual(pms.seen,('order','p','trusted-order'))
        self.assertEqual(calls[-1]['arguments']['readback'],{'order':{'id':'trusted-order','property_id':'p','version':3}})
        self.assertEqual(result['phase'],'awaiting_confirmation')

    def test_model_cannot_supply_authority_or_wrong_shapes(self):
        for args in ({'action':'x','approved':True}, {'action':{}}, {'action':''}):
            with self.assertRaises(ValueError): plugin.validate('execute',args)
        for data in ({'propertyId':'other'}, {'nested':{'actor':'owner'}}):
            with self.assertRaises(ValueError):
                plugin.validate('prepare',{'binding':'b','operation':'pms.command.CREATE_ORDER','input':data,'reason':{'code':'C','note':''}})
        with self.assertRaises(ValueError): plugin.validate('read',{'binding':'b','query':'unlisted'})

    def test_event_collection_requires_actual_matching_available_payment(self):
        args = {'binding':'b','operation':'pms.command.RECORD_COLLECTION','work_item':'w',
                'input':{'orderId':'o','method':'WECOM','transactionReference':'ref','amountMinor':100},
                'reason':{'code':'CUSTOMER_PAYMENT','note':''}}
        for override in ({'status':'MATCHED'}, {'reference':'other'}, {'amountMinor':200}, {'id':'other'}):
            calls=[]
            class Pms:
                def read(self,kind,prop,filters):
                    self.called=(kind,prop,filters)
                    return {'items':[dict(id='bill',status='AVAILABLE',kind='COLLECTION',reference='ref',amountMinor=100) | override]}
            def broker(request):
                calls.append(request['tool'])
                return {'ok':True,'result':{'bill_id':'bill','property':'property_a'}}
            pms=Pms()
            op=plugin.Operations(pms,broker,lambda:{})
            with self.assertRaises(ValueError): op.prepare(args)
            self.assertEqual(calls,['pms_event_context'])
            self.assertEqual(pms.called,('payments','property_a',{'billId':'bill','kind':'COLLECTION','status':'ALL','limit':1}))
        with self.assertRaises(ValueError):
            plugin.validate('prepare',dict(args,source_payment={'status':'AVAILABLE'}))

    def test_receipt_order_readback_uses_trusted_input_property(self):
        class Pms:
            def read(self,kind,prop,resource):
                self.called=(kind,prop,resource)
                return {'order':{'id':resource,'property_id':prop,'phone':'private'}}
        pms=Pms()
        calls=[]
        def broker(r):
            calls.append(r)
            return {'ok':True,'result':r['arguments']}
        op=plugin.Operations(pms,broker,lambda:{})
        result=op._finish('a','c',{'executionStatus':'EXECUTED','businessCommitted':True,'result':{'id':'order_1'}},{'propertyId':'p'})
        self.assertEqual(pms.called,('order','p','order_1'))
        self.assertNotIn('phone',result['readback']['order'])

    def test_receipt_stays_committed_when_readback_unavailable(self):
        class Pms:
            def read(self,*args,**kwargs): raise plugin.client.PmsError('pms_unavailable')
        op=plugin.Operations(Pms(),lambda r:{'ok':True,'result':r['arguments']},lambda:{})
        result=op._finish('a','c',{'executionStatus':'EXECUTED','result':{'id':'o'}},{'propertyId':'p'})
        self.assertEqual(result['result']['executionStatus'],'EXECUTED')
        self.assertIsNone(result['readback'])

    def test_resume_and_interrupted_draft_reprepare_without_execution(self):
        for entry in ('resume', 'recover'):
            calls=[]
            class Pms:
                def preview(self, command, data, **kwargs):
                    self.key=kwargs['key']
                    return {'preview':{'previewId':'fresh','commandType':command}}
            def broker(r):
                name=r['tool'];calls.append(name)
                result={'phase':'draft'}
                if name=='pms_claim_preview':
                    result={'operation':'pms.command.CHECK_IN','input':{'propertyId':'p','orderId':'o'},'preview_key':'fresh_key','correlation':'c','claim':'claim'}
                elif name=='pms_save_preview':
                    result={'phase':'awaiting_confirmation','confirmation_reused':False}
                return {'ok':True,'result':result}
            pms=Pms()
            result=plugin.Operations(pms,broker,lambda:{}).invoke(entry,{'action':'a'})
            self.assertEqual(result['result']['phase'],'awaiting_confirmation')
            self.assertFalse(result['result']['confirmation_reused'])
            self.assertEqual(pms.key,'fresh_key')
            self.assertEqual(calls[-2:],['pms_claim_preview','pms_save_preview'])
            self.assertNotIn('pms_claim_execute',calls)

    def test_official_host_capture_rejects_bot_internal_and_other_profile(self):
        source=types.SimpleNamespace(platform='wecom',chat_type='dm',chat_id='chat',user_id='staff',is_bot=False,profile='anan')
        event=types.SimpleNamespace(source=source,internal=False,text='确认方案 '+'a'*32,message_id='message')
        calls=[]
        def transport(request,**kwargs):
            calls.append((request,kwargs));return {'ok':True}
        with patch.dict('os.environ',{'QINTOPIA_FOUNDATION_GATEWAY_ID':'gateway'}):
            self.assertIsNone(plugin.host.capture(event,transport))
            self.assertEqual(calls[0][0]['trusted_context']['chat_type'],'direct')
            self.assertTrue(calls[0][1]['host'])
            for attr,val in [('is_bot',True),('profile','erhua')]:
                old=getattr(source,attr);setattr(source,attr,val)
                plugin.host.capture(event,transport);setattr(source,attr,old)
            event.internal=True;plugin.host.capture(event,transport)
        self.assertEqual(len(calls),1)

    def test_session_never_uses_fallback_without_bound_context(self):
        module=types.ModuleType('gateway.session_context')
        module.session_context_engaged=lambda:False
        module.get_session_env=lambda *args:'anan'
        with patch.dict(sys.modules,{'gateway':types.SimpleNamespace(session_context=module),'gateway.session_context':module}):
            with self.assertRaises(ValueError):plugin.host.session_context()

    def test_public_receipt_omits_private_nested_fields(self):
        result=plugin.public({'result':{'primaryGuest':{'nickname':'模拟','phone':'secret','documentNumber':'secret'},'notes':'private'},'execution_key':'secret'})
        self.assertEqual(result,{'result':{'primaryGuest':{'nickname':'模拟'}}})
        event=plugin.public({'schemaVersion':'pms.payments.v1','events':[{'eventId':'payment:b:DISCOVERED','sequence':'9007199254740993','eventType':'DISCOVERED','rawPayload':'private'}]})
        self.assertEqual(event,{'schemaVersion':'pms.payments.v1','events':[{'eventId':'payment:b:DISCOVERED','sequence':'9007199254740993','eventType':'DISCOVERED'}]})

if __name__=='__main__':unittest.main()
