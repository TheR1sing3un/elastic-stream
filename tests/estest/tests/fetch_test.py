from ducktape.tests.test import Test
from estest.services.pd import PD
from ducktape.mark.resource import cluster
from ducktape.mark import matrix

from estest.services.range_server import RangeServer
from estest.services.verifiable_producer import VerifiableProducer
from estest.services.verifiable_consumer import VerifiableConsumer
from estest.services.fetch import Fetch

class FetchTest(Test):
    def __init__(self, test_context):
        super(FetchTest, self).__init__(test_context=test_context)
    # @cluster(num_nodes=4)
    @matrix(rs_count=[1, 3], count=[1024], batch_size=[10])
    def test_fetch(self, rs_count, count, batch_size):
        pd = PD(self.test_context, num_nodes=rs_count)
        pd.start()
        rs = RangeServer(self.test_context, num_nodes=rs_count, pd=pd)
        rs.start()
        fetch = Fetch(self.test_context, num_nodes=1, pd=pd, replica=rs_count, count=count, batch_size=batch_size)
        fetch.start()

        pd.clean()
        rs.clean()
