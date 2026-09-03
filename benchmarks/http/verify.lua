response = function(status, headers, body)
    if status ~= 200 or body ~= "Hello, World!" then
        error("unexpected HTTP response: status=" .. status .. " body=" .. body)
    end
end
