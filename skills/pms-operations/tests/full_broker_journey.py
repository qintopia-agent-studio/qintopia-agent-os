"""Only source event and synthetic people are fixtures. Broker, plugin and PMS are real."""
import datetime
import importlib.util
import json
import os
from pathlib import Path
import sys
import urllib.request
from types import SimpleNamespace
from zoneinfo import ZoneInfo

if os.environ.get('ANAN_FULL_CHAIN') != '1': raise RuntimeError('explicit_full_chain_required')
sys.path.insert(0,os.environ['ANAN_HERMES_SOURCE'])
from gateway.session_context import set_session_vars, clear_session_vars
spec=importlib.util.spec_from_file_location('anan_full_plugin',Path(__file__).parents[1]/'__init__.py')
p=importlib.util.module_from_spec(spec);spec.loader.exec_module(p)
demo=json.loads(os.environ['ANAN_PMS_TEST_DEMO'])
binding=os.environ['ANAN_PMS_TEST_BINDING']
client=p.client.Client(os.environ['ANAN_PMS_TEST_BASE_URL'],demo['writeToken'],local_enabled=True)
ops=p.Operations(client)
os.environ['GREENPMS_BASE_URL']=os.environ['ANAN_PMS_TEST_BASE_URL']
os.environ['GREENPMS_API_TOKEN']=demo['writeToken']
from hermes_cli.plugins import PluginContext, PluginManager, parse_manifest_file
from tools.registry import registry
plugin_dir=Path(__file__).resolve().parents[1]
manifest=parse_manifest_file(plugin_dir/'plugin.yaml',plugin_dir,'project','')
assert manifest is not None
manager=PluginManager()
p.register(PluginContext(manifest,manager))
assert set(manifest.provides_tools)=={'qintopia_pms_'+key for key in p.SCHEMAS}
for tool in manifest.provides_tools: assert registry.get_entry(tool,scope=manager.scope_key) is not None

turn=0
def capture(text):
    global turn
    turn+=1
    event=SimpleNamespace(source=SimpleNamespace(platform='wecom',chat_type='dm',chat_id='synthetic_chat',user_id=os.environ['ANAN_PMS_TEST_SENDER'],is_bot=False,profile='anan'),internal=False,message_id=f'full_{turn}',text=text)
    assert manager._hooks["pre_gateway_dispatch"][0](event) is None
    set_session_vars(platform='wecom',chat_type='dm',chat_id=event.source.chat_id,user_id=event.source.user_id,message_id=event.message_id,profile='anan')

def invoke(name,args):
    result=json.loads(registry.get_entry('qintopia_pms_'+name,scope=manager.scope_key).handler(args))
    if not result['ok']:raise ValueError(result['error']['code'])
    return result['result']

def prepare(command,input,work=None):
    args={'binding':binding,'operation':'pms.command.'+command,'input':input,'reason':{'code':'CREATE_STANDARD_ORDER' if command=='CREATE_ORDER' else 'AGENT_HTTP_ACCEPTANCE','note':'' if command=='CREATE_ORDER' else 'Synthetic human request'}}
    if work:args['work_item']=work
    result=invoke('prepare',args)
    if result['phase']=='unknown': result=invoke('recover',{'action':result['action']})
    return result

today=datetime.datetime.now(ZoneInfo('Asia/Shanghai')).date()
arrival,departure=today.isoformat(),(today+datetime.timedelta(days=2)).isoformat()
capture('请查询可售房源和价格')
assert invoke('read',{'binding':binding,'query':'availability','filters':{'arrivalDate':arrival,'departureDate':departure,'unitKind':'ROOM'}})['units']
q=invoke('prepare',{'binding':binding,'operation':'pms.quote','input':{'inventoryUnitId':demo['roomId'],'stayType':'TRANSIENT','arrivalDate':arrival,'departureDate':departure,'pricingPolicyVersionId':demo['transientPolicyId']},'reason':{'code':'QUOTE','note':''}})
assert q['phase']=='completed',q
quote_id=q['result']['result']['quote']['quoteId']
# Explicit complete instruction arrives before PMS Preview. No extra approval prompt.
capture(f'请为「Synthetic Broker Guest」（昵称「模拟住客」）预订「101」整间，{arrival}入住，{departure}离店，1位住客，总价240.00元，企微渠道，无会员权益，不加其他安排。')
order_input={'quoteId':quote_id,'primaryGuest':{'fullName':'Synthetic Broker Guest','nickname':'模拟住客'},'bookingChannelCode':'WECOM'}
prepared=prepare('CREATE_ORDER',order_input)
assert prepared['confirmation_reused'] is True,prepared
created=invoke('execute',{'action':prepared['action']})
assert created['phase']=='completed' and created['readback']['order']['id'],created
order_id=created['readback']['order']['id']
application_work=os.environ['ANAN_PMS_TEST_APPLICATION_WORK']
capture('请核对这份入住申请和刚完成的订单')
link_args={'action':prepared['action'],'work_item':application_work}
proposal=invoke('link',link_args)
assert proposal['phase']=='awaiting_confirmation' and proposal['order']['id']==order_id,proposal
capture('确认关联')
linked=invoke('link',link_args)
assert linked['linked'] and linked['orderId']==order_id,linked
# Repeating the same message/plan obtains the persisted result, never another Confirm.
repeated=prepare('CREATE_ORDER',order_input)
assert repeated['action']==prepared['action'] and repeated['phase']=='completed'
try: invoke('execute',{'action':prepared['action']})
except ValueError: pass
else: raise AssertionError('duplicate execution claimed')
# A distinct collection needs a distinct human confirmation and does not imply arrival.
capture('登记客服已核对的模拟银行收款')
collection=prepare('RECORD_COLLECTION',{'orderId':order_id,'amountMinor':12000,'method':'BANK_TRANSFER','transactionReference':'SYNTHETIC-BROKER-ONE'},application_work)
assert not collection['confirmation_reused']
try: invoke('execute',{'action':collection['action']})
except ValueError: pass
else: raise AssertionError('missing human confirmation accepted')
capture('确认登记这笔收款')
collected=invoke('execute',{'action':collection['action']})
assert collected['phase']=='completed' and collected['readback']['order']['id']==order_id,collected
assert collected['readback']['order']['status']!='CHECKED_IN'
# A lost Confirm response must preserve the successful booking and recover only this collection.
capture(f'请为订单「{order_id}」登记银行转账收款120.00元，流水号「SYNTHETIC-BROKER-TWO」。')
second=prepare('RECORD_COLLECTION',{'orderId':order_id,'amountMinor':12000,'method':'BANK_TRANSFER','transactionReference':'SYNTHETIC-BROKER-TWO'})
assert second['confirmation_reused'] is True
class LoseResponse(p.client.Client):
    def confirm(self,*args,**kwargs):
        super().confirm(*args,**kwargs)
        raise p.client.PmsError('pms_outcome_unknown',outcome_unknown=True)
lost_ops=p.Operations(LoseResponse(os.environ['ANAN_PMS_TEST_BASE_URL'],demo['writeToken'],local_enabled=True))
lost=lost_ops.invoke('execute',{'action':second['action']})['result']
assert lost['phase']=='unknown',lost
assert invoke('status',{'action':prepared['action']})['phase']=='completed'
recovered=invoke('recover',{'action':second['action']})
assert recovered['phase']=='completed' and recovered['readback']['order']['id']==order_id,recovered

capture('请准备改期')
reschedule=prepare('RESCHEDULE_STAY',{'orderId':order_id,'newArrivalDate':arrival,'newDepartureDate':(today+datetime.timedelta(days=3)).isoformat()})
capture('确认改期')
assert invoke('execute',{'action':reschedule['action']})['phase']=='completed'

# Simulate a staff member taking over through the same PMS HTTP boundary used by UI.
capture('办理这位住客入住')
checkin=prepare('CHECK_IN',{'orderId':order_id})
invoke('handoff',{'action':checkin['action']})
ui_preview=client.preview('CHECK_IN',{'propertyId':demo['propertyId'],'orderId':order_id},key='ui_checkin_preview',correlation='ui_checkin')['preview']
ui_preview['propertyId']=demo['propertyId']
assert client.confirm(ui_preview,{'code':'STAFF_UI','note':'模拟人工接手'},key='ui_checkin',correlation='ui_checkin')['executionStatus']=='EXECUTED'
capture('已在PMS办理这个方案，订单号'+order_id)
manual=invoke('reconcile',{'action':checkin['action']})
assert manual['phase']=='manual_completed' and manual['readback']['order']['status']=='CHECKED_IN',manual
try:invoke('execute',{'action':checkin['action']})
except ValueError:pass
else:raise AssertionError('manually completed action reexecuted')
# Actual stay changes use independent human decisions and read back each result.
for command,params,text in [
    ('EXTEND_STAY',{'newDepartureDate':(today+datetime.timedelta(days=4)).isoformat()},'确认续住'),
    ('SHORTEN_STAY',{'newDepartureDate':departure},'确认缩短住宿'),
    ('MOVE_UNIT',{'newInventoryUnitId':demo['secondRoomId'],'effectiveDate':arrival},'确认换房'),
]:
    capture('请准备住宿调整：'+command)
    change=prepare(command,{'orderId':order_id,**params})
    if command=='SHORTEN_STAY':
        assert change['phase']=='preview_rejected',change
        continue
    assert change['phase']=='awaiting_confirmation',(command,change)
    capture(text)
    changed=invoke('execute',{'action':change['action']})
    assert changed['phase']=='completed' and changed['readback']['order']['id']==order_id,changed

# Early normal checkout is rejected by PMS; original preview key is resolved before cancelling.
capture('请核对现在是否可普通退房')
checkout=prepare('CHECK_OUT',{'orderId':order_id})
assert checkout['phase']=='preview_rejected',checkout

# Advance only this disposable PMS process's official test clock. No production
# time, order row, inventory or retained payment fixture is edited.
def advance(date):
    request=urllib.request.Request(os.environ['ANAN_PMS_TEST_BASE_URL']+'/_anan_test/clock',
        data=json.dumps({'instant':date+'T12:00:00+08:00'}).encode(),
        headers={'Content-Type':'application/json','x-anan-test-clock':os.environ['ANAN_PMS_TEST_CLOCK_TOKEN']},method='POST')
    with urllib.request.urlopen(request,timeout=10) as response: assert json.load(response)['updated']

# Match PMS's own shortening integration pattern: provision the stay through
# commands on yesterday's test clock, then shorten on the actual database day.
yesterday=(today-datetime.timedelta(days=1)).isoformat()
short_departure=(today+datetime.timedelta(days=1)).isoformat()
advance(yesterday)
capture('准备一笔跨营业日缩住的独立模拟住宿')
short_quote=invoke('prepare',{'binding':binding,'operation':'pms.quote','input':{'inventoryUnitId':demo['roomId'],'stayType':'TRANSIENT','arrivalDate':yesterday,'departureDate':(today+datetime.timedelta(days=3)).isoformat(),'pricingPolicyVersionId':demo['transientPolicyId']},'reason':{'code':'QUOTE','note':''}})
short_booking=prepare('CREATE_ORDER',{**order_input,'quoteId':short_quote['result']['result']['quote']['quoteId']})
capture('确认预订')
short_created=invoke('execute',{'action':short_booking['action']})
short_order=short_created['readback']['order']['id']
capture('请办理模拟到店')
short_checkin=prepare('CHECK_IN',{'orderId':short_order})
capture('确认已到店，办理入住')
assert invoke('execute',{'action':short_checkin['action']})['phase']=='completed'
advance(today.isoformat())
capture('请在次营业日缩短此住宿')
shortened=prepare('SHORTEN_STAY',{'orderId':short_order,'newDepartureDate':short_departure})
assert shortened['phase']=='awaiting_confirmation',shortened
capture('确认缩短住宿')
shortened=invoke('execute',{'action':shortened['action']})
assert shortened['phase']=='completed' and shortened['readback']['order']['id']==short_order,shortened
advance(short_departure)
capture('请在到期日办理正常退房')
normal=prepare('CHECK_OUT',{'orderId':short_order})
assert normal['phase']=='awaiting_confirmation',normal
capture('确认办理退房')
normal=invoke('execute',{'action':normal['action']})
assert normal['phase']=='completed' and normal['readback']['order']['status']=='CHECKED_OUT',normal

# A later stay for the same named guest is a separate order and matter.
advance(today.isoformat())
future_arrival=(today+datetime.timedelta(days=2)).isoformat()
future_departure=(today+datetime.timedelta(days=3)).isoformat()
capture('请预订同一住客的另一次未来住宿')
future_quote=invoke('prepare',{'binding':binding,'operation':'pms.quote','input':{'inventoryUnitId':demo['roomId'],'stayType':'TRANSIENT','arrivalDate':future_arrival,'departureDate':future_departure,'pricingPolicyVersionId':demo['transientPolicyId']},'reason':{'code':'QUOTE','note':''}})
if future_quote['phase'] in ('unknown','previewing'): future_quote=invoke('recover',{'action':future_quote['action']})
assert future_quote['phase']=='completed',future_quote
future=prepare('CREATE_ORDER',{**order_input,'quoteId':future_quote['result']['result']['quote']['quoteId']})
capture('确认预订')
future_result=invoke('execute',{'action':future['action']})
future_id=future_result['readback']['order']['id']
assert future_id!=order_id and invoke('status',{'action':future['action']})['work_item']!=invoke('status',{'action':prepared['action']})['work_item']
capture('取消这次未来预订，由客服直接在PMS接手')
cancel=prepare('CANCEL_ORDER',{'orderId':future_id})
assert cancel['phase']=='awaiting_confirmation',cancel
invoke('handoff',{'action':cancel['action']})
cancel_preview=client.preview('CANCEL_ORDER',{'propertyId':demo['propertyId'],'orderId':future_id},key='ui_future_cancel_preview',correlation='ui_future_cancel')['preview']
cancel_preview['propertyId']=demo['propertyId']
assert client.confirm(cancel_preview,{'code':'STAFF_UI','note':'模拟人工取消未来预订'},key='ui_future_cancel',correlation='ui_future_cancel')['executionStatus']=='EXECUTED'
capture('已在PMS办理这个方案，订单号'+future_id)
cancelled=invoke('reconcile',{'action':cancel['action']})
assert cancelled['phase']=='manual_completed' and cancelled['readback']['order']['status']=='CANCELLED',cancelled
try:invoke('execute',{'action':cancel['action']})
except ValueError:pass
else:raise AssertionError('manual cancellation replayed')
print('Additional actual PMS paths passed: next-day SHORTEN_STAY; due-date CHECK_OUT; separate future stay CANCEL_ORDER by simulated staff HTTP takeover, durable readback and no replay.')
print('Full local chain passed: original instruction reuse; natural confirmations; independent collections; dropped Confirm response recovery; reschedule/extend/move; same-day shorten and early checkout rejected; human PMS handoff readback; PMS early-checkout rejection without replay.')
clear_session_vars([])
