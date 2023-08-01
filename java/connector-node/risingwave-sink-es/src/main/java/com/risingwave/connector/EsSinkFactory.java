// Copyright 2023 RisingWave Labs
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

package com.risingwave.connector;

import com.fasterxml.jackson.databind.DeserializationFeature;
import com.fasterxml.jackson.databind.ObjectMapper;
import com.risingwave.connector.api.TableSchema;
import com.risingwave.connector.api.sink.SinkBase;
import com.risingwave.connector.api.sink.SinkFactory;
import com.risingwave.proto.Catalog;
import io.grpc.Status;
import java.io.IOException;
import java.util.Map;
import org.apache.http.HttpHost;
import org.elasticsearch.client.RequestOptions;
import org.elasticsearch.client.RestClient;
import org.elasticsearch.client.RestHighLevelClient;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;

public class EsSinkFactory implements SinkFactory {
    private static final Logger LOG = LoggerFactory.getLogger(EsSinkFactory.class);

    public SinkBase create(TableSchema tableSchema, Map<String, String> tableProperties) {
        ObjectMapper mapper = new ObjectMapper();
        EsSinkConfig config = mapper.convertValue(tableProperties, EsSinkConfig.class);
        return new EsSink(config, tableSchema);
    }

    @Override
    public void validate(
            TableSchema tableSchema,
            Map<String, String> tableProperties,
            Catalog.SinkType sinkType) {
        ObjectMapper mapper = new ObjectMapper();
        mapper.configure(DeserializationFeature.FAIL_ON_MISSING_CREATOR_PROPERTIES, true);
        EsSinkConfig config = mapper.convertValue(tableProperties, EsSinkConfig.class);

        // 1. check url
        HttpHost host;
        try {
            host = HttpHost.create(config.getUrl());
        } catch (IllegalArgumentException e) {
            throw Status.INVALID_ARGUMENT.withDescription(e.getMessage()).asRuntimeException();
        }

        // 2. check connection
        RestHighLevelClient client = new RestHighLevelClient(RestClient.builder(host));
        // Test connection
        try {
            boolean isConnected = client.ping(RequestOptions.DEFAULT);
            if (!isConnected) {
                throw Status.INVALID_ARGUMENT
                        .withDescription("Cannot connect to " + config.getUrl())
                        .asRuntimeException();
            }
        } catch (Exception e) {
            throw Status.INTERNAL.withDescription(e.getMessage()).asRuntimeException();
        }
        // 3. close client
        try {
            client.close();
        } catch (IOException e) {
            throw Status.INTERNAL.withDescription(e.getMessage()).asRuntimeException();
        }
    }
}
