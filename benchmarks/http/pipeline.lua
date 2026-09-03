local pipeline

init = function(args)
    local depth = tonumber(args[1]) or 16
    pipeline = wrk.format("GET", "/plaintext"):rep(depth)
end

request = function()
    return pipeline
end
